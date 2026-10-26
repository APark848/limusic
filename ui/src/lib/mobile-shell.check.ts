import {
	MOBILE_NAV_ROUTES,
	isMobileShell,
	showMobileMiniPlayer,
	zoomPolicy,
	type WindowChrome
} from './mobile-shell.ts';

function ok(condition: boolean, message: string): void {
	if (!condition) throw new Error(message);
}

ok(isMobileShell('mobile'), 'mobile chrome selects the phone shell');
for (const chrome of ['off', 'on', 'overlay'] satisfies WindowChrome[]) {
	ok(!isMobileShell(chrome), `${chrome} chrome preserves the desktop shell`);
}
ok(
	JSON.stringify(MOBILE_NAV_ROUTES) === JSON.stringify(['/', '/search', '/library']),
	'the phone navigation exposes Home, Search, and Library routes'
);
ok(showMobileMiniPlayer('mobile', true, false), 'an active mobile track shows the mini-player');
ok(!showMobileMiniPlayer('mobile', false, false), 'no track means no mobile mini-player');
ok(!showMobileMiniPlayer('mobile', true, true), 'full Now Playing replaces the mini-player');
ok(!showMobileMiniPlayer('off', true, false), 'desktop never renders the mobile mini-player');
ok(zoomPolicy('off', false) === 'pending', 'zoom waits for native chrome resolution');
ok(zoomPolicy('mobile', true) === 'fixed', 'mobile uses a fixed native-app zoom');
for (const chrome of ['off', 'on', 'overlay'] satisfies WindowChrome[]) {
	ok(zoomPolicy(chrome, true) === 'desktop', `${chrome} chrome keeps desktop zoom available`);
}

console.log('ok');
