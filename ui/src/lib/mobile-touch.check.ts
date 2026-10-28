// @ts-expect-error Self-checks run directly under Node; the app tsconfig intentionally excludes Node types.
import { readFileSync } from 'node:fs';

function ok(condition: boolean, message: string): void {
	if (!condition) throw new Error(message);
}

const css = readFileSync(new URL('../routes/layout.css', import.meta.url), 'utf8');
const search = readFileSync(new URL('./components/SearchSuggest.svelte', import.meta.url), 'utf8');
const settings = readFileSync(new URL('./components/SettingsDialog.svelte', import.meta.url), 'utf8');
const nowPlaying = readFileSync(new URL('./components/NowPlaying.svelte', import.meta.url), 'utf8');
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
ok(settings.includes('flex min-h-0 min-w-0 flex-1 flex-col'), 'settings content has a shrinkable flex ancestor');
ok(settings.includes('min-h-0 min-w-0 flex-1 overflow-y-auto overflow-x-hidden'), 'settings owns vertical scrolling');
ok(settings.includes('mobile-settings-scroll touch-pan-y overscroll-contain'), 'mobile settings contain touch scrolling');
ok(app.includes('viewport-fit=cover'), 'the viewport exposes iPhone safe-area insets');

console.log('ok');
