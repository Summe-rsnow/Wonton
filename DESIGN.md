# Wonton — design standards

A single-window desktop tool for Antigravity accounts: add accounts, read their
model quotas, and start Antigravity behind a proxy the app sets.

## Product

- Primary task: see which account still has quota, after adding accounts once.
- Object: an account (email) and its model quotas.
- Always available: add account, refresh, proxy target, start Antigravity.
- States: no accounts, loading quota, stale quota, refresh error, login expired,
  proxy invalid, Antigravity not found.
- Targets: desktop (Windows today; macOS/Linux share the design). No mobile target.

## Visual language

- Minimal, quiet surfaces; hairline borders; no shadows except on the dialog.
- One accent (`primary`) for the one committal action of a region; `destructive`
  only for removal and failures; `success`/`warning` only for quota states.
- Radius: `theme().radius` everywhere (8pt); no pills except the status dot.
- Type: platform UI font, `text-sm` body, `text-xs` metadata, `text-base` titles.
- Spacing: the 4pt scale only (`gap_1` … `gap_6`, `p_4`).
- Density: desktop compact — controls 32pt, rows 28–40pt.

### Tokens

All colors come from the Gemini-inspired light/dark theme (`theme.json`):
`background`, `foreground`, `card`, `muted`, `muted-foreground`, `accent`,
`accent-foreground`, `secondary`, `border`, `primary`, `primary-foreground`,
`destructive`, `destructive-foreground`, plus `success`, `warning`, and `scrim`
(a scrim carries its own alpha, so modal backdrops remain theme tokens too).

### Motion tokens

| Token | Value | For |
|---|---|---|
| instant | 120ms | hover wash, color change |
| fast | 180ms | press |
| base | 240ms | selection, glyph swap |

Press: scale 0.97 (`transition_transform`, 150ms, soft settle).
Hover: `accent` wash, `transition_colors`, 120ms.
Reduced motion: every motion below lands instantly (`reduce_motion()` is honored by
`Motion` and by `transition_*`).

## Motion inventory

| Change | Motion |
|---|---|
| Button pressed (add, refresh, start, save) | scale 0.97, `fast`, soft settle |
| Button hover | `accent` wash, `instant`, ease-out |
| Add-account dialog opens | `FadeInUp`-equivalent: rise 8pt and fade, `slow` (360ms), ease-out |
| Add-account dialog closes | none (native dismissal), content unmounted |
| Dialog tab changes | one sliding indicator (native `tabs()` indicator), `base`, ease-in-out |
| Quota bar value updates | width transition `base`, ease-in-out |
| Account row appears after add | rise 8pt and fade in, `slow` |
| Account row removed | none (unmounted); the list closes the gap instantly |
| About dialog opens | `FadeInUp` (`motion().entering(MotionPreset::FadeInUp)`), `slow`, ease-out |
| About dialog closes | none (native dismissal), content unmounted |
| Update check state changes | button disables with `"正在检测…"`, status text updates in place |
| Tab content switch (网页授权 / 手动 Token / 本机导入) | instant swap; the panels are small and adjacent |
| Notice text appears (saved / started / error) | floating bottom toast, fade in `instant`, auto-dismisses after 3s (6s for errors) |
| Quota refreshing | the row's timestamp text changes; no spinner (the fetch is fast and the row keeps its data) |
| Account selection checkbox toggled | fill & border color transition, `instant`, ease-out |
| Batch import modal opens | `FadeInUp`-equivalent: rise 8pt and fade, `slow` (360ms), ease-out |
| Batch import progress bar updates | width transition `base`, ease-in-out |
| Account card drag reorder | dragged card lifts above siblings (`z_index` 20, `shadow_lg`, `primary` border) and tracks pointer 1:1 via `AnimatedValue`; sibling cards glide smoothly (`140ms` ease-out) into vacated slots as midpoints cross; dropped card settles (`140ms` ease-out) and persists |
| Reduced motion | all of the above instant; nothing depends on motion to be understood |

## The `ui` layer

`src/ui/`: `icons` (24×24 stroke vector SVG icon definitions plus `assets/logo-mark.svg` and `assets/tray-icon.png`), `button` (`ButtonKind`
primary / secondary / ghost / destructive, `IconKind` primary / secondary / toolbar_danger / toolbar_launch / launch /
subtle / danger, `external_link`, `hint_tooltip`, `icon_action`, `icon_action_disabled`,
`icon_action_with_tip`, and `segmented_icon`),
`input` (text field, checkbox, shared vertically centered `toggle_switch`, and `radio_option`), `surface` (card, divider,
empty state, status badge, tier badge, notice toast), `quota` (single-row pool bar
with inline `5H` and `周` windows; `Argon` and Free show `周` only), and
`progress_bar`. The dialog's tabs use `nsh_base`'s `tabs` with the app's
indicator style. Features compose these and never style a control themselves.

## Layout

One workspace: an immersive window header integrated into the hidden system title bar (`48pt`, draggable via `window_drag`, reserving `title_bar().left`/`right` for native window controls; brand mark `assets/logo-mark.svg` + title + version badge `vX.Y.Z` + subtitle + 3-mode icon appearance switcher `跟随系统`/`浅色模式`/`深色模式` + About/Update icon button), a compact HTTP proxy & action toolbar (enable/disable toggle switch, host, port, test proxy, save proxy grouped on the left; non-text icon buttons on the right ordered as import, export [selected or all], refresh [selected or all], delete [selected, disabled when none selected], add-account | launch Classic, launch IDE, launch CLI), a fixed-height (`34pt`) selection summary bar, then the draggable compact account list (each row with a vector drag grip, selection checkbox, subscription tier badge `ULTRA` / `PRO` / `FREE`, the three quota pools ordered `Gemini` → `Claude [4.6/5.5] / GPT` → `Gemini Argon` (`周` only at bottom, when eligible) showing `5H` and `周` side by side on one row with concise durations like `4小时38分` / `6天23小时`, and non-text vector icon actions for Classic / IDE / CLI client launch, export, refresh, and delete — which opens a two-step confirmation dialog — all with hover tooltips), which is the page's main region and the only scroller. Desktop system tray (`assets/tray-icon.png`) restores/focuses on click and exposes show, refresh-all, about, close-button preference (`每次询问` / `最小化到托盘` / `直接退出应用`), and quit; pressing close with `每次询问` opens the close modal with a `不再提示` checkbox. Notices float at the bottom via `notice_toast` without shifting layout. Minimum window 720×560; the toolbar wraps cleanly on narrower viewports.
