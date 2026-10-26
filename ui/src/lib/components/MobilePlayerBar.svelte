<script lang="ts">
	import { HugeiconsIcon } from '@hugeicons/svelte';
	import { MusicNote01Icon, PauseIcon, PlayIcon } from '@hugeicons/core-free-icons';
	import * as api from '$lib/api';
	import { np, playback } from '$lib/player.svelte';
	import { thumb } from '$lib/thumb';
	import { t } from '$lib/i18n.svelte';
</script>

<div class="relative z-30 flex min-h-16 shrink-0 items-center gap-1 border-t bg-card px-2 py-1.5">
	<button
		type="button"
		onclick={() => (np.open = true)}
		class="flex min-h-12 min-w-0 flex-1 items-center gap-3 rounded-xl px-1 text-left active:bg-muted"
		aria-label={t('player.open_player')}
	>
		{#if playback.now?.thumbnail}
			<img
				src={thumb(playback.now.thumbnail, 120)}
				alt=""
				class="h-12 w-12 shrink-0 rounded-lg object-cover"
			/>
		{:else}
			<span
				class="flex h-12 w-12 shrink-0 items-center justify-center rounded-lg bg-muted text-muted-foreground"
			>
				<HugeiconsIcon icon={MusicNote01Icon} class="h-5 w-5" />
			</span>
		{/if}
		<span class="min-w-0 flex-1">
			<span class="block truncate text-sm font-semibold">{playback.now?.title}</span>
			<span class="block truncate text-xs text-muted-foreground">{playback.now?.artists}</span>
		</span>
	</button>
	<button
		type="button"
		onclick={() => api.togglePause()}
		class="flex h-12 w-12 shrink-0 items-center justify-center rounded-full active:bg-muted"
		aria-label={playback.paused ? t('player.play') : t('player.pause')}
	>
		<HugeiconsIcon
			icon={PauseIcon}
			altIcon={PlayIcon}
			showAlt={playback.paused}
			class="h-6 w-6"
		/>
	</button>
</div>
