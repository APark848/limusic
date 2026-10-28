// @ts-expect-error Self-checks run directly under Node; the app tsconfig intentionally excludes Node types.
import { readFileSync } from 'node:fs';

function ok(condition: boolean, message: string): void {
	if (!condition) throw new Error(message);
}

const css = readFileSync(new URL('../routes/layout.css', import.meta.url), 'utf8');
const search = readFileSync(new URL('./components/SearchSuggest.svelte', import.meta.url), 'utf8');
const settings = readFileSync(new URL('./components/SettingsDialog.svelte', import.meta.url), 'utf8');
const nowPlaying = readFileSync(new URL('./components/NowPlaying.svelte', import.meta.url), 'utf8');
const trackRow = readFileSync(new URL('./components/TrackRow.svelte', import.meta.url), 'utf8');
const mediaCard = readFileSync(new URL('./components/MediaCard.svelte', import.meta.url), 'utf8');
const app = readFileSync(new URL('../app.html', import.meta.url), 'utf8');

ok(!css.includes('.mobile-shell .rail {\n\ttouch-action: pan-x;'), 'shelves must allow vertical page gestures');
ok(css.includes('.mobile-shell .mobile-search-suggest'), 'mobile suggestions have scoped styling');
ok(css.includes('overflow-y: auto;') && css.includes('overscroll-behavior: contain;'), 'mobile suggestions scroll independently');
ok(search.includes('mobile-search-suggest'), 'the suggestion panel uses the mobile scroll class');
ok(
	search.includes("win.chrome === 'mobile'") && search.includes('document.activeElement.blur()'),
	'choosing a mobile search result dismisses the keyboard without changing desktop focus'
);
ok(nowPlaying.includes('swipeOffset >= 96'), 'mobile Now Playing has a distance dismiss threshold');
ok(nowPlaying.includes('velocity >= 0.65'), 'mobile Now Playing has a velocity dismiss threshold');
ok(nowPlaying.includes('[data-np-scroll]'), 'scrolling tabs are excluded from swipe dismissal');
ok(
	nowPlaying.includes('button:not([data-np-artwork])') &&
		nowPlaying.includes('suppressArtworkToggleUntil'),
	'artwork drags may dismiss Now Playing without turning a swipe into play/pause'
);
ok(
	nowPlaying.includes('[data-np-seek]') && nowPlaying.includes('seekFromPointer'),
	'the mobile seek target owns pointer scrubbing instead of the parent dismiss gesture'
);
ok(css.includes('min-height: 44px;') && css.includes('touch-action: none;'), 'mobile seek has a native-size isolated touch target');
ok(
	trackRow.includes("win.chrome === 'mobile'") &&
		trackRow.includes("compact || mobile") &&
		trackRow.includes("class=\"track-row") &&
		trackRow.includes(": 'hover:bg-accent/10'"),
	'mobile track rows do not reveal hover-only descendants before their primary click'
);
ok(
	mediaCard.includes("win.chrome === 'mobile'") &&
		mediaCard.includes("mobile ? '' : 'group'") &&
		mediaCard.includes("mobile ? 'opacity-100'"),
	'mobile song cards expose controls without consuming a first tap to establish hover'
);
ok(settings.includes('flex min-h-0 min-w-0 flex-1 flex-col'), 'settings content has a shrinkable flex ancestor');
ok(settings.includes('min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden'), 'settings owns vertical scrolling');
ok(settings.includes('mobile-settings-scroll touch-pan-y overscroll-contain'), 'mobile settings contain touch scrolling');
ok(app.includes('viewport-fit=cover'), 'the viewport exposes iPhone safe-area insets');

console.log('ok');
