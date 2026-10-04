//! Read-only local Matter client. Commission with the probe once, then load its
//! private identity from MATTER.TXT. No setup code or switching commands here.
pub mod network;

use crate::{
    supervisor,
    ticker::{health::Who, power::PowerConfig},
};
use core::num::NonZeroU8;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_futures::select::{Either3, select3};
use embassy_net::{
    Stack,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_rp::clocks::RoscRng;
use embassy_time::{Duration, Timer, with_timeout};
use network::MatterUdp;
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use rs_matter::Matter;
use rs_matter::cert::{MAX_CERT_TLV_AND_ASN1_LEN, r#gen::VALID_FOREVER};
use rs_matter::crypto::{CanonAeadKey, CanonPkcSecretKey, Crypto, Rng as _, SecretKey, SharedRand, SigningSecretKey, default_crypto};
use rs_matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::im::{AttrPath, AttrResp, GenericPath, client::ImClient as _};
use rs_matter::onboard::{
    cac::{IcacGenerator, RcacGenerator},
    noc::NocGenerator,
};
use rs_matter::tlv::{FromTLV, TLVArray};
use rs_matter::transport::exchange::Exchange;
use rs_matter::transport::network::{
    NoNetwork,
    mdns::{
        MDNS_IPV4_BROADCAST_ADDR, MDNS_IPV6_BROADCAST_ADDR,
        builtin::{BuiltinMdns, Host},
    },
};
use rs_matter::utils::init::InitMaybeUninit;
use static_cell::StaticCell;

const FABRIC_ID: u64 = 0x5032_4d11_0000_0001;
const CONTROLLER_NODE_ID: u64 = 0x2350;
const POWER_CLUSTER: u32 = 0x0090;
const ACTIVE_POWER: u32 = 0x0008;
const READ_TIMEOUT: Duration = Duration::from_secs(20);
/// Set after the first OTA check. Recovery never creates this task.
pub static START: AtomicBool = AtomicBool::new(false);
pub static FIRST_DONE: AtomicBool = AtomicBool::new(false);
#[derive(Clone, Copy)]
pub enum Update {
    Sample(Option<i64>),
    Failed,
    Unsupported,
}

pub async fn run(stack: Stack<'static>, config: PowerConfig, publish: fn(Update), ready: fn() -> bool) -> ! {
    supervisor::park(Who::Matter);
    while !START.load(Ordering::Relaxed) {
        Timer::after_millis(250).await;
    }
    supervisor::beat(Who::Matter);
    static MATTER: StaticCell<Matter<'static>> = StaticCell::new();
    let matter = MATTER.uninit().init_with(Matter::init(&TEST_DEV_DET, TEST_DEV_COMM, &TEST_DEV_ATT, 5540));
    let mut seed = [0; 32];
    RoscRng.fill_bytes(&mut seed);
    let rng = SharedRand::new(ChaCha20Rng::from_seed(seed));
    let crypto = default_crypto(&rng, DAC_PRIVKEY);
    let fabric = match identity(matter, &crypto, config.seed) {
        Ok(fabric) => fabric,
        Err(error) => {
            log::warn!("MATTER_IDENTITY_FAILED code={:?}", error.code());
            publish(Update::Failed);
            FIRST_DONE.store(true, Ordering::Relaxed);
            supervisor::park(Who::Matter);
            loop {
                Timer::after_secs(60).await;
            }
        }
    };
    loop {
        supervisor::park(Who::Matter);
        while !ready() {
            Timer::after_secs(1).await;
        }
        supervisor::beat(Who::Matter);
        let result = crate::noinline::noinline(cycle(stack, matter, &crypto, fabric, config.device_node, publish, ready)).await;
        if let Err(error) = result {
            publish(Update::Failed);
            log::warn!("MATTER_RETRY code={:?}", error.code());
        }
        FIRST_DONE.store(true, Ordering::Relaxed);
        // All transport futures are dropped before reset. The fabric stays.
        let _ = matter.reset_transport();
        supervisor::beat(Who::Matter);
        Timer::after_secs(5).await;
    }
}

fn identity<C: Crypto>(matter: &Matter<'_>, crypto: &C, seed: [u8; 32]) -> Result<NonZeroU8, Error> {
    // Separate stable identity randomness from fresh session randomness above.
    let rng = SharedRand::new(ChaCha20Rng::from_seed(seed));
    let identity_crypto = default_crypto(&rng, DAC_PRIVKEY);
    let mut rcac_buf = [0; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut rcac_gen = RcacGenerator::new(&mut rcac_buf);
    let (rcac_priv, rcac) = rcac_gen.generate(&identity_crypto, FABRIC_ID, VALID_FOREVER)?;
    let mut icac_buf = [0; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut icac_gen = IcacGenerator::new(&mut icac_buf);
    let (icac_priv, icac) = icac_gen.generate(&identity_crypto, rcac_priv.reference(), rcac, VALID_FOREVER)?;
    drop(rcac_priv);
    let controller_key = identity_crypto.generate_secret_key()?;
    let mut csr_buf = [0; 256];
    let csr = controller_key.csr(&mut csr_buf)?;
    let mut canon = CanonPkcSecretKey::new();
    controller_key.write_canon(&mut canon)?;
    let mut noc_buf = [0; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut noc_gen = NocGenerator::create(icac_priv.reference(), rcac, icac, &mut noc_buf)?;
    let noc = noc_gen.generate(&identity_crypto, csr, CONTROLLER_NODE_ID, &[], VALID_FOREVER)?;
    let mut ipk = CanonAeadKey::new();
    identity_crypto.rand()?.fill_bytes(ipk.access_mut());
    matter.with_state(|state| {
        state
            .fabrics
            .add(crypto, canon.reference(), rcac, noc, icac, Some(ipk.reference()), 0xfff1, CONTROLLER_NODE_ID)
            .map(|fabric| fabric.fab_idx())
    })
}

/// Small dedicated UDP buffers leave the single OTA/TLS workspace available.
async fn cycle<C: Crypto>(
    stack: Stack<'static>,
    matter: &Matter<'_>,
    crypto: &C,
    fabric: NonZeroU8,
    node: u64,
    publish: fn(Update),
    ready: fn() -> bool,
) -> Result<(), Error> {
    let ipv4 = stack.config_v4().ok_or(ErrorCode::NoNetworkInterface)?.address.address();
    let ipv6 = stack.config_v6().ok_or(ErrorCode::NoNetworkInterface)?.address.address();
    stack.join_multicast_group(MDNS_IPV4_BROADCAST_ADDR).map_err(|_| ErrorCode::NoNetworkInterface)?;
    stack.join_multicast_group(MDNS_IPV6_BROADCAST_ADDR).map_err(|_| ErrorCode::NoNetworkInterface)?;
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx = [0; 4096];
    let mut tx = [0; 2048];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx, &mut tx_meta, &mut tx);
    socket.bind(5540).map_err(|_| ErrorCode::NoNetworkInterface)?;
    let mut mdns_rx_meta = [PacketMetadata::EMPTY; 4];
    let mut mdns_tx_meta = [PacketMetadata::EMPTY; 4];
    let mut mdns_rx = [0; 4096];
    let mut mdns_tx = [0; 2048];
    let mut mdns_socket = UdpSocket::new(stack, &mut mdns_rx_meta, &mut mdns_rx, &mut mdns_tx_meta, &mut mdns_tx);
    mdns_socket.bind(5353).map_err(|_| ErrorCode::NoNetworkInterface)?;
    mdns_socket.set_hop_limit(Some(255));
    let mut mdns = BuiltinMdns::new();
    let addresses = [ipv6];
    let host = Host { hostname: "pico2w-ticker", ip: ipv4, ipv6: &addresses };
    log::info!("MATTER_START operational CASE");
    match select3(
        matter.run(crypto, MatterUdp(&socket), MatterUdp(&socket), NoNetwork),
        mdns.run(MatterUdp(&mdns_socket), MatterUdp(&mdns_socket), &host, Some(ipv4), Some(0), matter, crypto),
        flow(matter, crypto, fabric, node, publish, ready),
    )
    .await
    {
        Either3::First(result) | Either3::Second(result) | Either3::Third(result) => result,
    }
}

async fn flow<C: Crypto>(
    matter: &Matter<'_>,
    crypto: &C,
    fabric: NonZeroU8,
    node: u64,
    publish: fn(Update),
    ready: fn() -> bool,
) -> Result<(), Error> {
    // Discover the endpoint and establish CASE in the same lookup/read.
    let endpoint = with_timeout(READ_TIMEOUT, read(matter, crypto, fabric, node, None)).await.map_err(|_| ErrorCode::RxTimeout)??;
    let ReadValue::Endpoint(Some(endpoint)) = endpoint else {
        publish(Update::Unsupported);
        FIRST_DONE.store(true, Ordering::Relaxed);
        log::warn!("MATTER_UNSUPPORTED cluster=0x0090");
        for _ in 0..60 {
            supervisor::beat(Who::Matter);
            Timer::after_secs(5).await;
        }
        return Ok(());
    };
    log::info!("MATTER_CASE_READY endpoint={endpoint}");
    let mut failures = 0;
    loop {
        supervisor::beat(Who::Matter);
        if !ready() {
            return Err(ErrorCode::NoNetworkInterface.into());
        }
        let result = with_timeout(READ_TIMEOUT, read(matter, crypto, fabric, node, Some(endpoint)))
            .await
            .map_err(|_| Error::from(ErrorCode::RxTimeout))
            .and_then(|r| r);
        match result {
            Ok(ReadValue::Power(value)) => {
                publish(Update::Sample(value));
                failures = 0;
            }
            Ok(_) => return Err(ErrorCode::InvalidData.into()),
            Err(error) => {
                publish(Update::Failed);
                failures += 1;
                if failures >= 3 {
                    return Err(error);
                }
                log::warn!("MATTER_READ_FAILED code={:?}", error.code());
            }
        }
        FIRST_DONE.store(true, Ordering::Relaxed);
        supervisor::beat(Who::Matter);
        Timer::after_secs(5).await;
    }
}

enum ReadValue {
    Endpoint(Option<u16>),
    Power(Option<i64>),
}
async fn read<C: Crypto>(matter: &Matter<'_>, crypto: &C, fabric: NonZeroU8, node: u64, endpoint: Option<u16>) -> Result<ReadValue, Error> {
    let (cluster, attr) = if endpoint.is_some() { (POWER_CLUSTER, ACTIVE_POWER) } else { (0x001d, 1) };
    let paths = [AttrPath::from_gp(&GenericPath::new(endpoint, Some(cluster), Some(attr)))];
    let exchange = Exchange::initiate(matter, crypto, fabric, node).await?;
    let mut chunk = exchange.read_with(|b| b.attr_requests_from(&paths)?.fabric_filtered(true)?.end()).await?;
    let mut found = None;
    let mut power = None;
    let mut descriptor_read = false;
    loop {
        {
            let response = chunk.response()?;
            if let Some(reports) = response.attr_reports {
                for report in reports.iter() {
                    if let AttrResp::Data(data) = report? {
                        if data.path.cluster != Some(cluster) || data.path.attr != Some(attr) {
                            continue;
                        }
                        if endpoint.is_some() {
                            if data.path.endpoint == endpoint {
                                power = Some(if data.data.null().is_ok() { None } else { Some(data.data.i64()?) });
                            }
                        } else {
                            descriptor_read = true;
                            let mut has_power = false;
                            if let Ok(array) = TLVArray::<u32>::from_tlv(&data.data) {
                                for cluster in array.iter() {
                                    has_power |= cluster? == POWER_CLUSTER;
                                }
                            } else if data.path.list_index.is_some() {
                                has_power = data.data.u32()? == POWER_CLUSTER;
                            }
                            if has_power && found.is_none() {
                                found = data.path.endpoint;
                            }
                        }
                    }
                }
            }
        }
        match chunk.complete().await? {
            Some(next) => chunk = next,
            None => break,
        }
    }
    if endpoint.is_some() {
        power.map(ReadValue::Power).ok_or_else(|| ErrorCode::InvalidData.into())
    } else if descriptor_read {
        Ok(ReadValue::Endpoint(found))
    } else {
        Err(ErrorCode::InvalidData.into())
    }
}
