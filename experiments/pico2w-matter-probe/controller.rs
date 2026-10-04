use crate::{config, network::MatterUdp};
use core::net::{Ipv4Addr, Ipv6Addr};
use core::num::NonZeroU8;
use embassy_futures::select::{Either3, select3};
use embassy_net::{
    Stack,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_rp::clocks::RoscRng;
use embassy_time::{Duration, Timer, with_timeout};
use rand_chacha::ChaCha20Rng;
use rand_core::SeedableRng;
use rs_matter::Matter;
use rs_matter::cert::{MAX_CERT_TLV_AND_ASN1_LEN, MAX_CERT_TLV_LEN, r#gen::VALID_FOREVER};
use rs_matter::crypto::{
    CanonAeadKey, CanonPkcSecretKey, Crypto, Rng as _, SecretKey, SharedRand, SigningSecretKey,
    default_crypto,
};
use rs_matter::dm::devices::test::{DAC_PRIVKEY, TEST_DEV_ATT, TEST_DEV_COMM, TEST_DEV_DET};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::im::{AttrPath, AttrResp, GenericPath, client::ImClient as _};
use rs_matter::onboard::{
    CommissionOptions, Commissioner,
    cac::{IcacGenerator, RcacGenerator},
    noc::NocGenerator,
};
use rs_matter::tlv::{FromTLV, TLVArray};
use rs_matter::transport::exchange::Exchange;
use rs_matter::transport::network::{
    NoNetwork,
    mdns::{
        CommissionableFilter, MDNS_IPV4_BROADCAST_ADDR, MDNS_IPV6_BROADCAST_ADDR,
        builtin::{BuiltinMdns, Host},
    },
};
use rs_matter::utils::init::InitMaybeUninit;
use static_cell::StaticCell;

const FABRIC_ID: u64 = 0x50324d1100000001;
const CONTROLLER_NODE_ID: u64 = 0x2350;
const DEVICE_NODE_ID: u64 = 0x110;
const POWER_CLUSTER: u32 = 0x0090;
const ACTIVE_POWER: u32 = 0x0008;

pub async fn run(
    stack: Stack<'static>,
    ipv4: Ipv4Addr,
    ipv6: Ipv6Addr,
    hardware_rng: &mut RoscRng,
) -> Result<(), Error> {
    stack
        .join_multicast_group(MDNS_IPV4_BROADCAST_ADDR)
        .map_err(|_| ErrorCode::NoNetworkInterface)?;
    stack
        .join_multicast_group(MDNS_IPV6_BROADCAST_ADDR)
        .map_err(|_| ErrorCode::NoNetworkInterface)?;
    let mut rx_meta = [PacketMetadata::EMPTY; 4];
    let mut tx_meta = [PacketMetadata::EMPTY; 4];
    let mut rx_buf = [0u8; 8192];
    let mut tx_buf = [0u8; 8192];
    let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);
    socket
        .bind(5540)
        .map_err(|_| ErrorCode::NoNetworkInterface)?;
    let mut mdns_rx_meta = [PacketMetadata::EMPTY; 8];
    let mut mdns_tx_meta = [PacketMetadata::EMPTY; 4];
    let mut mdns_rx = [0u8; 8192];
    let mut mdns_tx = [0u8; 4096];
    let mut mdns_socket = UdpSocket::new(
        stack,
        &mut mdns_rx_meta,
        &mut mdns_rx,
        &mut mdns_tx_meta,
        &mut mdns_tx,
    );
    mdns_socket
        .bind(5353)
        .map_err(|_| ErrorCode::NoNetworkInterface)?;
    mdns_socket.set_hop_limit(Some(255));

    let mut seed = [0u8; 32];
    hardware_rng.fill_bytes(&mut seed);
    let session_rng = SharedRand::new(ChaCha20Rng::from_seed(seed));
    let crypto = default_crypto(&session_rng, DAC_PRIVKEY);
    static MATTER: StaticCell<Matter<'static>> = StaticCell::new();
    let matter = MATTER.uninit().init_with(Matter::init(
        &TEST_DEV_DET,
        TEST_DEV_COMM,
        &TEST_DEV_ATT,
        5540,
    ));
    let mut mdns = BuiltinMdns::new();
    let ipv6_addresses = [ipv6];
    let host = Host {
        hostname: "pico2w-matter-probe",
        ip: ipv4,
        ipv6: &ipv6_addresses,
    };
    log::info!("MATTER_READY local_port=5540");
    match select3(
        matter.run(&crypto, MatterUdp(&socket), MatterUdp(&socket), NoNetwork),
        mdns.run(
            MatterUdp(&mdns_socket),
            MatterUdp(&mdns_socket),
            &host,
            Some(ipv4),
            Some(0),
            matter,
            &crypto,
        ),
        flow(matter, &crypto),
    )
    .await
    {
        Either3::First(result) | Either3::Second(result) | Either3::Third(result) => result,
    }
}

async fn flow<C: Crypto>(matter: &Matter<'_>, crypto: &C) -> Result<(), Error> {
    // Generate the same private CA/key/IPK on each boot from the saved random seed.
    // This RNG is used only for identity creation; session randomness is fresh.
    let identity_rng = SharedRand::new(ChaCha20Rng::from_seed(config::CONTROLLER_SEED));
    let identity_crypto = default_crypto(&identity_rng, DAC_PRIVKEY);
    log::info!("CONTROLLER_IDENTITY generating stable credentials");
    let mut rcac_buf = [0u8; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut rcac_gen = RcacGenerator::new(&mut rcac_buf);
    let (rcac_priv, rcac) = rcac_gen.generate(&identity_crypto, FABRIC_ID, VALID_FOREVER)?;
    let mut icac_buf = [0u8; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut icac_gen = IcacGenerator::new(&mut icac_buf);
    let (icac_priv, icac) =
        icac_gen.generate(&identity_crypto, rcac_priv.reference(), rcac, VALID_FOREVER)?;
    drop(rcac_priv);
    let controller_key = identity_crypto.generate_secret_key()?;
    let mut csr_buf = [0u8; 256];
    let csr = controller_key.csr(&mut csr_buf)?;
    let mut canon = CanonPkcSecretKey::new();
    controller_key.write_canon(&mut canon)?;
    let mut noc_buf = [0u8; MAX_CERT_TLV_AND_ASN1_LEN];
    let mut noc_gen = NocGenerator::create(icac_priv.reference(), rcac, icac, &mut noc_buf)?;
    let controller_noc = noc_gen.generate(
        &identity_crypto,
        csr,
        CONTROLLER_NODE_ID,
        &[],
        VALID_FOREVER,
    )?;
    let mut ipk = CanonAeadKey::new();
    identity_crypto.rand()?.fill_bytes(ipk.access_mut());
    let fab_idx = matter.with_state(|state| {
        state
            .fabrics
            .add(
                crypto,
                canon.reference(),
                rcac,
                controller_noc,
                icac,
                Some(ipk.reference()),
                0xfff1,
                CONTROLLER_NODE_ID,
            )
            .map(|f| f.fab_idx())
    })?;
    log::info!("CONTROLLER_IDENTITY_READY");

    // Establish CASE on the first operational lookup. Resolving once just to
    // check presence and resolving again for the first read loses the answer
    // with this mDNS backend. Reads can reuse the authenticated session instead.
    let existing = match with_timeout(
        Duration::from_secs(30),
        Exchange::initiate(matter, crypto, fab_idx, DEVICE_NODE_ID),
    )
    .await
    {
        Ok(Ok(exchange)) => {
            drop(exchange);
            true
        }
        Ok(Err(e)) if e.code() == ErrorCode::NotFound => false,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(ErrorCode::RxTimeout.into()),
    };
    if existing {
        log::info!("OPERATIONAL_SESSION_READY CASE=authenticated reusing existing fabric");
    } else {
        let filter = CommissionableFilter {
            short_discriminator: Some(config::SHORT_DISCRIMINATOR),
            commissioning_mode_only: true,
            ..Default::default()
        };
        log::info!("DISCOVERY_WAIT: looking for the setup-code discriminator");
        let peer = loop {
            match matter
                .transport()
                .browse_commissionable(&filter, &[], 10000)
                .await
            {
                Ok((address, _)) => break address,
                Err(e) if e.code() == ErrorCode::NotFound => {
                    log::info!("DISCOVERY_WAIT: P110M commissioning advertisement not found");
                    Timer::after_secs(5).await;
                }
                Err(e) => return Err(e),
            }
        };
        log::info!("COMMISSIONABLE_FOUND address={peer}");
        let mut scratch = [0u8; MAX_CERT_TLV_LEN];
        let mut commissioner =
            Commissioner::new(matter, crypto, fab_idx, &mut noc_gen, &mut scratch);
        // Upstream currently lacks DAC/PAA/DCL validation. PASE still authenticates
        // the entered setup code. This opt-in is limited to this local experiment.
        let options = CommissionOptions {
            fail_safe_secs: 120,
            allow_test_attestation: true,
        };
        log::info!("COMMISSION_START attestation_validation=unimplemented-in-upstream");
        let phase1 = with_timeout(
            Duration::from_secs(60),
            commissioner.commission(
                peer,
                config::PASSCODE,
                &options,
                DEVICE_NODE_ID,
                VALID_FOREVER,
            ),
        )
        .await
        .map_err(|_| ErrorCode::RxTimeout)??;
        log::info!(
            "COMMISSION_PHASE1_OK device_fabric_index={}",
            phase1.fabric_index
        );
        with_timeout(
            Duration::from_secs(60),
            commissioner.complete_via_case(&phase1),
        )
        .await
        .map_err(|_| ErrorCode::RxTimeout)??;
        log::info!("COMMISSION_COMPLETE CASE=authenticated");
    }

    let basic = [
        path(Some(0), 0x0028, 2),  // VendorID
        path(Some(0), 0x0028, 3),  // ProductName
        path(Some(0), 0x0028, 4),  // ProductID
        path(Some(0), 0x0028, 8),  // HardwareVersionString
        path(Some(0), 0x0028, 10), // SoftwareVersionString
    ];
    read(matter, crypto, fab_idx, &basic, ReadKind::Basic).await?;
    let endpoint = read(
        matter,
        crypto,
        fab_idx,
        &[path(None, 0x001d, 1)],
        ReadKind::Servers,
    )
    .await?
    .ok_or(ErrorCode::NotFound)?;
    log::info!("POWER_ENDPOINT_FOUND endpoint={endpoint} cluster=0x0090 attribute=0x0008");
    loop {
        let requested = [path(Some(endpoint), POWER_CLUSTER, ACTIVE_POWER)];
        match with_timeout(
            Duration::from_secs(20),
            read(matter, crypto, fab_idx, &requested, ReadKind::Power),
        )
        .await
        {
            Ok(Ok(_)) => (),
            Ok(Err(e)) => log::warn!("POWER_READ_FAILED: {e:?}"),
            Err(_) => log::warn!("POWER_READ_TIMEOUT"),
        }
        Timer::after_secs(5).await;
    }
}

fn path(endpoint: Option<u16>, cluster: u32, attr: u32) -> AttrPath {
    AttrPath::from_gp(&GenericPath::new(endpoint, Some(cluster), Some(attr)))
}

#[derive(Clone, Copy)]
enum ReadKind {
    Basic,
    Servers,
    Power,
}

async fn read<C: Crypto>(
    matter: &Matter<'_>,
    crypto: &C,
    fabric: NonZeroU8,
    paths: &[AttrPath],
    kind: ReadKind,
) -> Result<Option<u16>, Error> {
    let exchange = Exchange::initiate(matter, crypto, fabric, DEVICE_NODE_ID).await?;
    let mut chunk = exchange
        .read_with(|b| b.attr_requests_from(paths)?.fabric_filtered(true)?.end())
        .await?;
    let mut power_endpoint = None;
    loop {
        {
            let response = chunk.response()?;
            if let Some(reports) = response.attr_reports {
                for report in reports.iter() {
                    match report? {
                        AttrResp::Status(s) => log::warn!(
                            "ATTR_STATUS endpoint={:?} cluster={:?} attribute={:?} status={:?}",
                            s.path.endpoint,
                            s.path.cluster,
                            s.path.attr,
                            s.status
                        ),
                        AttrResp::Data(data) => match kind {
                            ReadKind::Basic => {
                                if let Ok(text) = data.data.utf8() {
                                    log::info!(
                                        "BASIC_INFO attribute=0x{:04x} value={text}",
                                        data.path.attr.unwrap_or(0)
                                    );
                                } else if let Ok(value) = data.data.u32() {
                                    log::info!(
                                        "BASIC_INFO attribute=0x{:04x} value={value}",
                                        data.path.attr.unwrap_or(0)
                                    );
                                }
                            }
                            ReadKind::Servers => {
                                let mut has_power = false;
                                if let Ok(array) = TLVArray::<u32>::from_tlv(&data.data) {
                                    for cluster in array.iter() {
                                        if cluster? == POWER_CLUSTER {
                                            has_power = true;
                                        }
                                    }
                                } else if data.path.list_index.is_some() {
                                    has_power = data.data.u32()? == POWER_CLUSTER;
                                }
                                log::info!(
                                    "DESCRIPTOR endpoint={:?} electrical_power={has_power}",
                                    data.path.endpoint
                                );
                                if has_power {
                                    power_endpoint = data.path.endpoint;
                                }
                            }
                            ReadKind::Power => {
                                if data.data.null().is_ok() {
                                    log::info!(
                                        "ACTIVE_POWER_NULL endpoint={:?}",
                                        data.path.endpoint
                                    );
                                } else {
                                    let mw = data.data.i64()?;
                                    let magnitude = mw.unsigned_abs();
                                    let sign = if mw < 0 { "-" } else { "" };
                                    log::info!(
                                        "ACTIVE_POWER endpoint={} raw_mW={mw} watts={sign}{}.{:03} W",
                                        data.path.endpoint.unwrap_or(0),
                                        magnitude / 1000,
                                        magnitude % 1000
                                    );
                                }
                            }
                        },
                    }
                }
            }
        }
        match chunk.complete().await? {
            Some(next) => chunk = next,
            None => break,
        }
    }
    Ok(power_endpoint)
}
