"""設定画面の幅・LCD の固定文字・ダイアログをブラウザで検証する。

mock_server.py を起動してから実行 (実機への POST はしない):
    python tools/settings-mock/check_layout.py --out .local/settings-layout
Playwright の Chromium がなければ --channel msedge 等を指定できる。
PNG は数値検査に加えて目視で確認すること。
"""
import argparse
import json
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright

AUDIT = """() => {
  const issues = [], lcd = document.querySelector('#lcd');
  const box = e => e.getBoundingClientRect();
  const visible = e => e.getClientRects().length && getComputedStyle(e).visibility !== 'hidden';
  if (document.documentElement.scrollWidth > innerWidth + 1) issues.push('page scrolls horizontally');
  for (const e of document.querySelectorAll('.photo .meta, .photo .ctl, button, .dialog')) {
    if (visible(e) && e.scrollWidth > e.clientWidth + 1) issues.push('overflow: ' + (e.id || e.className));
  }
  for (const e of document.querySelectorAll('.photo .meta')) {
    if (box(e).width < 90) issues.push('photo filename column too narrow');
  }
  const note = document.querySelector('.lcd-note'), nr = box(note), lr = box(lcd);
  if (nr.x > lr.right && nr.width < 220) issues.push('preview note too narrow');
  // DOM Range はフォントの余白まで含む。実際の字形の範囲を測り、
  // 混在する秒のサイズと 180 度回転も各テキストノードへ反映する。
  const ctx = document.createElement('canvas').getContext('2d');
  const rotated = lcd.style.transform.includes('180');
  if (lcd.classList.contains('power-graph')) {
    const p = box(lcd.querySelector('.power')), info = box(lcd.querySelector('.info-panel'));
    if (Math.abs(p.height-info.height)>1) issues.push('graph/clock tiles differ in height');
    const gap = rotated ? p.left-info.right : info.left-p.right;
    if (gap < lr.width*.3-2) issues.push('central background gap is narrower than 120/400');
  }
  if (lcd.classList.contains('power-focus')) {
    if (getComputedStyle(lcd.querySelector('.power-value')).textAlign!=='right') issues.push('power value is not right aligned');
    const panel = lcd.querySelector('.info-panel'), style = getComputedStyle(panel);
    const widest = Math.max(...Array.from(panel.querySelectorAll('.clock, .date, .weather-values, #lcd-wx')).filter(visible).map(e => box(e).width));
    const padding = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
    if (Math.abs(box(panel).width - widest - padding) > 1) issues.push('info tile has unused horizontal space');
  }
  const inkBox = e => {
    const walker = document.createTreeWalker(e, NodeFilter.SHOW_TEXT);
    let b = {left:Infinity,top:Infinity,right:-Infinity,bottom:-Infinity}, node;
    while ((node=walker.nextNode())) {
      if (!node.textContent.trim()) continue;
      const style = getComputedStyle(node.parentElement);
      ctx.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
      ctx.letterSpacing = style.letterSpacing === 'normal' ? '0px' : style.letterSpacing;
      const m = ctx.measureText(node.textContent);
      const range = document.createRange(); range.selectNodeContents(node);
      const r = range.getBoundingClientRect();
      const left = rotated ? lr.left+lr.right-r.right : r.left;
      const bottom = rotated ? lr.top+lr.bottom-r.top : r.bottom;
      const baseline = bottom - m.fontBoundingBoxDescent;
      let a = {left:left-m.actualBoundingBoxLeft, right:left+m.actualBoundingBoxRight,
               top:baseline-m.actualBoundingBoxAscent, bottom:baseline+m.actualBoundingBoxDescent};
      if (rotated) a = {left:lr.left+lr.right-a.right, right:lr.left+lr.right-a.left,
                        top:lr.top+lr.bottom-a.bottom, bottom:lr.top+lr.bottom-a.top};
      b = {left:Math.min(b.left,a.left),top:Math.min(b.top,a.top),right:Math.max(b.right,a.right),bottom:Math.max(b.bottom,a.bottom)};
    }
    return b;
  };
  const labels = [];
  const icon = lcd.querySelector('.weather-icon');
  if (visible(icon)) {
    const r = box(icon), b = box(lcd.querySelector('.info-panel'));
    if (r.left < b.left || r.right > b.right || r.top < b.top || r.bottom > b.bottom) issues.push('weather icon clips');
    labels.push({text:'weather icon',x0:r.left,y0:r.top,x1:r.right,y1:r.bottom});
  }
  if (lcd.classList.contains('power-graph')) {
    const number = lcd.querySelector('.power-number'), n = inkBox(number), clock = inkBox(lcd.querySelector('.clock'));
    const metadata = inkBox(lcd.querySelector('.power small')), plot = box(lcd.querySelector('.power-trend'));
    const scale = lr.width/400;
    // Compare in logical orientation, even when the whole preview is rotated.
    const top = b => rotated ? lr.top+lr.bottom-b.bottom : b.top;
    const bottom = b => rotated ? lr.top+lr.bottom-b.top : b.bottom;
    if (/[0-9]/.test(number.textContent) && Math.abs(top(n)-top(clock)) > scale+.5) issues.push('power/clock rows are misaligned');
    if (top(metadata)-bottom(n) < 2*scale-.5) issues.push('metadata too close to power number');
    if (top(plot)-bottom(metadata) < scale-.5) issues.push('plot too close to metadata');
  }
  for (const e of lcd.querySelectorAll('.clock, .date, .place, .temp, #lcd-rain, #lcd-wx, .power small, .power-value, .power-axis, .power-range span, .status-detail span')) {
    if (!visible(e)) continue;
    const r = inkBox(e);
    const owner = e.closest('.power') ||
      (lcd.classList.contains('power-focus') && e.matches('.clock, .date, .temp, #lcd-rain, #lcd-wx') ? lcd.querySelector('.info-panel') : e.closest('.card')) || lcd;
    const b = box(owner);
    if (r.left < b.left - 1 || r.right > b.right + 1 || r.top < b.top - 1 || r.bottom > b.bottom + 1) {
      issues.push('LCD clips: ' + e.textContent.trim());
    }
    labels.push({text:e.textContent.trim(), x0:r.left, y0:r.top, x1:r.right, y1:r.bottom});
  }
  for (let i=0; i<labels.length; i++) for (const a of labels.slice(0, i)) {
    const b = labels[i];
    if (Math.min(a.x1,b.x1)-Math.max(a.x0,b.x0) > 1 && Math.min(a.y1,b.y1)-Math.max(a.y0,b.y0) > 1) {
      issues.push('LCD overlap: ' + a.text + ' / ' + b.text);
    }
  }
  return issues;
}"""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--url', default='http://127.0.0.1:8089/')
    parser.add_argument('--out', type=Path, default=Path('.local/settings-layout'))
    parser.add_argument('--channel', default=None)
    args = parser.parse_args()
    # 保存・写真追加も検証するので、接続先はローカルの mock に限る。
    if urlparse(args.url).hostname not in ('127.0.0.1', 'localhost', '::1'):
        parser.error('--url must point to the local mock server')
    args.out.mkdir(parents=True, exist_ok=True)
    failures, errors, cases = [], [], 0

    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True, channel=args.channel)
        context = browser.new_context(viewport={'width':1280, 'height':900})
        page = context.new_page()
        page.set_default_timeout(8000)
        page.on('pageerror', lambda err: errors.append(str(err)))
        page.on('console', lambda msg: errors.append(msg.text) if msg.type=='error' else None)
        page.clock.set_fixed_time('2026-12-31T14:59:59Z')
        page.route('https://geocoding-api.open-meteo.com/**', lambda route: route.fulfill(json={
            'results':[{'name':'サン・マルタン・ド・ベルヴィル', 'admin1':'オーヴェルニュ＝ローヌ＝アルプ地域圏',
                        'country':'フランス', 'latitude':45.381, 'longitude':6.505, 'timezone':'Europe/Paris'}]
        }))
        page.goto(args.url, wait_until='networkidle')
        page.wait_for_function("document.querySelector('#f-place').value && document.querySelectorAll('.photo canvas').length > 0")
        assert page.locator('#lcd-rain').inner_text() == '40%' # Status API fixture reaches the preview.
        # 長い日付・天気、負の気温で固定文字の配置を確認。
        page.evaluate("document.querySelector('#lcd-temp').textContent='-99.9°'; document.querySelector('#lcd-wx').textContent='激しいにわか雨'")

        def check(name, rain='100%', temperature='-99.9°', condition='激しいにわか雨', code=82):
            nonlocal cases
            cases += 1
            page.locator('#lcd-temp').evaluate('(e, text) => e.textContent = text', temperature)
            page.locator('#lcd-wx').evaluate('(e, text) => e.textContent = text', condition)
            page.locator('#lcd-rain').evaluate('(e, text) => e.textContent = text', rain)
            page.evaluate('(code) => paintWeatherIcon(code, 23)', code)
            for issue in page.evaluate(AUDIT):
                failures.append(f'{name}: {issue}')

        def full(name):
            vp = page.viewport_size
            page.set_viewport_size({'width':vp['width'], 'height':page.evaluate('document.documentElement.scrollHeight')})
            page.screenshot(path=str(args.out / f'{name}.png'))
            page.set_viewport_size(vp)

        widths = [320, 390, 520, 768, 860, 861, 900, 1024, 1280, 1920]
        for width in widths:
            page.set_viewport_size({'width':width, 'height':844 if width<520 else 900})
            for mode, layout in [('normal','glass'), ('normal','dock'), ('normal','classic'), ('large','glass'), ('graph','glass')]:
                page.select_option('#f-power_display', mode)
                page.locator('label:has(#l-' + layout + ')').click()
                for status in ['auto', 'full']:
                    page.locator('label:has(#st-' + status + ')').click()
                    for rotation in ['0','180']:
                        page.select_option('#f-rotate', rotation)
                        name = f'{width}-{mode}-{layout}-{status}-{rotation}'
                        check(name)
                        if width in [390,900,1280] and rotation=='0':
                            page.locator('#lcd').screenshot(path=str(args.out / f'lcd-{name}.png'))
            page.select_option('#f-rotate', '0')
            page.locator('label:has(#st-auto)').click()
            if width in [320,390,768,900,1280]:
                full(f'settings-{width}')
                page.locator('#panel-photos').screenshot(path=str(args.out / f'photos-{width}.png'))
                page.locator('#panel-display').screenshot(path=str(args.out / f'display-{width}.png'))

        for width in [390,1280]:
            page.set_viewport_size({'width':width, 'height':844})
            for minutes in ['1','5','30','60','360','1440']:
                page.select_option('#f-power_minutes', minutes)
                check(f'{width}-range-{minutes}')
                assert page.locator('#lcd-power-range').inner_text() == page.locator('#f-power_minutes option:checked').inner_text()
            for rain in ['0%', '100%', '--%']:
                check(f'{width}-rain-{rain}', rain)
            for mode in ['normal','large','graph']:
                page.select_option('#f-power_display', mode)
                for value in ['0.0','3680.0','-3680.0','--']:
                    page.locator('.power-number').evaluate('(e,v) => e.textContent=v', value)
                    check(f'{width}-{mode}-{value}')
            page.locator('.power-number').evaluate("e => e.textContent='343.3'")

        # A short weather row shrinks the tile; a longer condition wins until it is hidden.
        for width in [390,1280]:
            page.set_viewport_size({'width':width,'height':844})
            for mode in ['large','graph']:
                page.select_option('#f-power_display', mode)
                for rotation in ['0','180']:
                    page.select_option('#f-rotate', rotation)
                    page.locator('label:has(#st-auto)').click()
                    check(f'{width}-{mode}-cloud-{rotation}', '40%', '21.4°', 'くもり', 3)
                    if rotation == '0':
                        page.locator('#lcd').screenshot(path=str(args.out/f'lcd-content-{width}-{mode}-cloud.png'))
                    check(f'{width}-{mode}-condition-{rotation}', '0%', '0.0°')
                    normal_width = page.locator('.info-panel').bounding_box()['width']
                    page.locator('label:has(#st-full)').click()
                    check(f'{width}-{mode}-hidden-condition-{rotation}', '0%', '0.0°')
                    assert page.locator('.info-panel').bounding_box()['width'] < normal_width - 1
                    if rotation == '0':
                        page.locator('#lcd').screenshot(path=str(args.out/f'lcd-content-{width}-{mode}-detail.png'))
        page.select_option('#f-rotate','0')
        page.locator('label:has(#st-auto)').click()

        # 保存の帯が折り返しても末尾と通知を隠さない。
        for width in [320,390,900]:
            page.set_viewport_size({'width':width,'height':720})
            page.fill('#f-place','神奈川')
            page.select_option('#f-rotate','180')
            page.select_option('#f-slide','600')
            page.locator('#panel-photos').scroll_into_view_if_needed()
            page.evaluate('window.scrollTo(0, document.documentElement.scrollHeight)')
            page.wait_for_function("document.querySelector('#panel-photos').getBoundingClientRect().bottom <= document.querySelector('.savebar').getBoundingClientRect().top")
            check(f'{width}-dirty-bottom')
            page.screenshot(path=str(args.out / f'savebar-{width}.png'))

        page.set_viewport_size({'width':320,'height':720})
        page.click('#btn-save')
        page.wait_for_selector('#dlg-code:not([hidden])')
        page.fill('#code-input','123456')
        check('access-code-320')
        submit = page.locator('#code-form button[type=submit]').bounding_box()
        codebox = page.locator('#code-input').bounding_box()
        assert abs(submit['x']-codebox['x'])<1 and abs(submit['width']-codebox['width'])<1
        page.screenshot(path=str(args.out / 'access-code.png'))
        page.click('#code-form button[type=submit]')
        page.wait_for_function("document.querySelector('#btn-save').disabled && document.querySelector('#save-msg').textContent==='変更はありません'")
        saved = page.request.get(args.url.rstrip('/') + '/api/settings').json()
        assert saved['place']=='神奈川' and saved['rotate']==180 and saved['power_display']=='graph' and saved['slide']==600
        assert saved['power_minutes']==1440
        page.reload(wait_until='networkidle')
        page.wait_for_function("document.querySelector('#f-place').value==='神奈川'")
        assert page.locator('#f-rotate').input_value()=='180'
        assert page.locator('#f-power_minutes').input_value()=='1440'
        page.select_option('#f-rotate','0')

        page.fill('#city-q','Saint Martin')
        page.click('#city-go')
        page.wait_for_selector('#city-results button')
        check('long-search-result')
        page.locator('#panel-region').screenshot(path=str(args.out / 'search.png'))
        page.click('#city-results button')
        assert len(page.locator('#f-place').input_value().encode('utf-8'))<=32
        page.click('#btn-revert')

        # 追加・切り抜き・コード・送信、削除の確認画面も狭い幅で検証。
        photo = Path(__file__).resolve().parents[1] / 'ui-sim/samples/SUNSET.BMP'
        page.set_input_files('#file', photo)
        page.wait_for_selector('#dlg-crop:not([hidden])')
        page.check('#crop-guide-on')
        assert 33 < page.locator('#crop-guides i').first.evaluate('e => parseFloat(e.style.width)') < 35
        for width,height in [(320,720),(390,844),(667,320),(844,390),(1280,900)]:
            page.set_viewport_size({'width':width,'height':height})
            check(f'crop-{width}x{height}')
            page.locator('#crop-ok').scroll_into_view_if_needed()
            page.screenshot(path=str(args.out / f'crop-{width}.png'))
        page.set_viewport_size({'width':390,'height':844})
        page.fill('#crop-name','LAYOUT.BMP')
        page.click('#crop-ok')
        page.wait_for_selector('#dlg-crop',state='hidden')
        page.wait_for_selector('.photo:has-text("LAYOUT.BMP")')
        check('uploaded-photo')
        order = page.locator('.photo .n').all_text_contents()
        uploaded = order.index('LAYOUT.BMP')
        page.locator('.photo:has-text("LAYOUT.BMP") button[aria-label="上へ"]').click()
        assert page.locator('.photo .n').all_text_contents()[uploaded-1]=='LAYOUT.BMP'
        page.locator('.photo:has-text("LAYOUT.BMP") button[aria-label="下へ"]').click()
        assert page.locator('.photo .n').all_text_contents()==order
        page.locator('.photo:has-text("LAYOUT.BMP") input[type=checkbox]').uncheck()
        assert page.locator('.photo.off:has-text("LAYOUT.BMP")').count()==1
        page.locator('.photo:has-text("LAYOUT.BMP") input[type=checkbox]').check()
        check('photo-controls')
        page.locator('#panel-photos').screenshot(path=str(args.out / 'photos-uploaded.png'))
        page.locator('.photo:has-text("LAYOUT.BMP") button.danger').click()
        page.wait_for_selector('#dlg-confirm:not([hidden])')
        check('confirm-390')
        page.screenshot(path=str(args.out / 'confirm.png'))
        page.click('#confirm-no')
        page.wait_for_selector('#dlg-confirm',state='hidden')
        assert page.locator('.photo:has-text("LAYOUT.BMP")').count()==1
        context.close()
        browser.close()

    result = {'cases':cases,'failures':failures,'page_errors':errors}
    (args.out/'checks.json').write_text(json.dumps(result,ensure_ascii=False,indent=2),encoding='utf-8')
    if failures or errors:
        unique = sorted(set(f.split(': ',1)[1] for f in failures))
        print(f'FAILED: {len(failures)} findings in {cases} cases. Details: {args.out / "checks.json"}')
        print(json.dumps({'findings':unique,'page_errors':errors},ensure_ascii=False,indent=2))
        raise SystemExit(1)
    print(f'PASS: {cases} layout cases, save/reload/upload/reorder/toggle/cancel, no browser errors; PNGs: {args.out}')


if __name__=='__main__':
    main()
