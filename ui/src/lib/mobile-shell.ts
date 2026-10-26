export type WindowChrome = 'off' | 'on' | 'overlay' | 'mobile';

export const MOBILE_NAV_ROUTES = ['/', '/search', '/library'] as const;

export function isMobileShell(chrome: WindowChrome): boolean {
	return chrome === 'mobile';
}

export function zoomPolicy(chrome: WindowChrome, ready: boolean): 'pending' | 'fixed' | 'desktop' {
	if (!ready) return 'pending';
	return isMobileShell(chrome) ? 'fixed' : 'desktop';
}

export function showMobileMiniPlayer(
	chrome: WindowChrome,
	hasTrack: boolean,
	nowPlayingOpen: boolean
): boolean {
	return isMobileShell(chrome) && hasTrack && !nowPlayingOpen;
}
