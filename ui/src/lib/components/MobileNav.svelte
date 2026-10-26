<script lang="ts">
	import { page } from '$app/state';
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import {
		Home01Icon,
		Search01Icon,
		LibraryIcon,
		Settings01Icon
	} from '@hugeicons/core-free-icons';
	import { MOBILE_NAV_ROUTES } from '$lib/mobile-shell';
	import { ui } from '$lib/player.svelte';
	import { t } from '$lib/i18n.svelte';

	const icons = [Home01Icon, Search01Icon, LibraryIcon];
	const labels = $derived([t('nav.home'), t('nav.search'), t('nav.library')]);
	const active = (href: string) =>
		href === '/' ? page.url.pathname === '/' : page.url.pathname.startsWith(href);
</script>

<nav
	aria-label="Mobile navigation"
	class="relative z-30 grid shrink-0 grid-cols-4 border-t bg-card/95 px-1 pt-1 text-muted-foreground"
	style="padding-right:max(env(safe-area-inset-right),0.25rem);padding-bottom:max(env(safe-area-inset-bottom),0.25rem);padding-left:max(env(safe-area-inset-left),0.25rem)"
>
	{#each MOBILE_NAV_ROUTES as href, i (href)}
		<a
			{href}
			aria-current={active(href) ? 'page' : undefined}
			class="flex min-h-14 min-w-0 flex-col items-center justify-center gap-0.5 rounded-xl px-1 text-[11px] font-medium transition-colors active:bg-muted {active(
				href
			)
				? 'text-primary'
				: 'text-muted-foreground'}"
		>
			<HugeiconsIcon icon={icons[i]} class="h-5 w-5" />
			<span class="truncate">{labels[i]}</span>
		</a>
	{/each}
	<button
		type="button"
		onclick={() => (ui.settingsOpen = true)}
		class="flex min-h-14 min-w-0 flex-col items-center justify-center gap-0.5 rounded-xl px-1 text-[11px] font-medium transition-colors active:bg-muted"
	>
		<HugeiconsIcon icon={Settings01Icon} class="h-5 w-5" />
		<span class="truncate">{t('nav.settings')}</span>
	</button>
</nav>
