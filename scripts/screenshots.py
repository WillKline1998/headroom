"""Screenshots of the UI in browser-preview mode (vite on :1420)."""
from playwright.sync_api import sync_playwright

OUT = "/Users/willkline/Projects/headroom/docs/screenshots"
with sync_playwright() as p:
    b = p.chromium.launch(channel="chrome", headless=True)
    for scheme in ("light", "dark"):
        ctx = b.new_context(viewport={"width": 340, "height": 260}, device_scale_factor=2, color_scheme=scheme)
        pg = ctx.new_page()
        errs = []
        pg.on("pageerror", lambda e: errs.append(str(e)))
        pg.goto("http://localhost:1420/?preview=widget")
        pg.wait_for_timeout(1500)
        pg.locator(".widget").screenshot(path=f"{OUT}/widget_{scheme}.png")
        print(scheme, "errors:", errs[:3])
    for account in ("enterprise", "apikey"):
        ctx = b.new_context(viewport={"width": 340, "height": 300}, device_scale_factor=2)
        pg = ctx.new_page()
        pg.goto(f"http://localhost:1420/?preview=widget&account={account}")
        pg.wait_for_timeout(1500)
        pg.locator(".widget").screenshot(path=f"{OUT}/widget_{account}.png")
    for scheme in ("light", "dark"):
        ctx = b.new_context(viewport={"width": 760, "height": 640}, device_scale_factor=2, color_scheme=scheme)
        for tab in ("limits", "models", "settings"):
            pg = ctx.new_page()
            errs = []
            pg.on("pageerror", lambda e: errs.append(str(e)))
            pg.goto(f"http://localhost:1420/?preview=details#details/{tab}")
            pg.wait_for_timeout(1500)
            pg.screenshot(path=f"{OUT}/details_{tab}_{scheme}.png", full_page=True)
            print(tab, scheme, "errors:", errs[:3])
    b.close()
