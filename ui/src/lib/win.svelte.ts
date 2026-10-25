// Shared window-frame state: the resize borders hide when maximized, and the root container drops
// its rounded corners. `chrome` says whether the compositor is drawing the frame instead of us.
// One listener, initialized once by the root layout.
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getSettings, onUiVisible } from '$lib/api';
import { setUiVisible } from '$lib/theme.svelte';
import { invoke } from '@tauri-apps/api/core';

/** Who draws the window frame. `off` = our custom titlebar owns it (the default), `on` = the
 *  compositor does (the "system title bar" setting, Linux/Windows), `overlay` = macOS traffic
 *  lights float over our bar, and `mobile` = there is no desktop window frame to manage.
 *  Anything but `off` hides our window buttons and corner rounding. Derived in Rust
 *  (`native_chrome` in commands.rs) so the SPA needs no user-agent platform check. */
export const win = $state({
	maximized: false,
	chrome: 'off' as 'off' | 'on' | 'overlay' | 'mobile'
});

let started = false;

export function initWin(): () => void {
	if (started) return () => {};
	started = true;
	const w = getCurrentWindow();
	let unResize: Promise<() => void> | undefined;
	// Desktop creates the window hidden (tauri.conf.json) so the window-state plugin can restore the
	// saved size before anything is on screen. iOS overrides that config with an immediately visible
	// opaque window and skips every desktop window-management operation below.
	//
	// Never swallow this one. The reveal used to be `w.show()`, which is ACL-gated, and a silent
	// catch here hid a missing `core:window:allow-show` for a week: it fell through to lib.rs's
	// safety net, so every launch sat on an empty desktop while the tray and the media keys already
	// worked (#122). `show_main` also keeps the window in the tray on a login launch with "Start
	// minimized to tray" on, and says which it did, so the page knows whether anyone can see it.
	// Settled before the desktop reveal, so the window never flashes rounded corners and a second set
	// of window buttons on its way to the system frame. The catch still reveals desktop if settings
	// fail, or a backend hiccup would leave the app with no window at all (#122).
	const startDesktopWindow = () => {
		invoke<boolean>('show_main')
			.then((visible) => setUiVisible(visible))
			.catch((e) => console.error('window show failed', e));
		const sync = () =>
			w
				.isMaximized()
				.then((m) => (win.maximized = m))
				.catch(() => {});
		sync();
		unResize = w.onResized(sync);
	};
	getSettings()
		.then((s) => {
			if (s.native_chrome === 'mobile') {
				win.chrome = 'mobile';
				setUiVisible(true);
				void invoke('frontend_ready').catch((e) => console.error('frontend ready failed', e));
				return;
			}
			if (s.native_chrome === 'on' || s.native_chrome === 'overlay') win.chrome = s.native_chrome;
			startDesktopWindow();
		})
		.catch(startDesktopWindow);
	// Hidden to the tray is not a state the page can see for itself (WebKitGTK keeps reporting the
	// document as visible), and work done for a hidden window is memory the web process keeps until
	// it is shown again. See `theme.svelte.ts`.
	const unVis = onUiVisible(setUiVisible);
	return () => {
		unResize?.then((un) => un());
		unVis.then((u) => u());
	};
}
