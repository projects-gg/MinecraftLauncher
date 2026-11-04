// Shared design system for Projects Launcher and Projects Studio (rules: docs/design.md).
import "./fonts";

export {
  Chip,
  EmptyState,
  Kbd,
  PixelBrackets,
  PlayerHead,
  Progress,
  Skeleton,
  SwapLabel,
} from "./components/bits";
export { type Brand, BrandMark } from "./components/brand-mark";
export { Button, type ButtonProps, IconButton, type IconButtonProps } from "./components/button";
export {
  CONTENT_SPRITES,
  ContentIcon,
  type ContentIconKind,
  type SpriteSize,
  type SpriteTone,
  spritePaths,
  spriteSize,
} from "./components/content-icon";
export { countdownTickMs } from "./components/countdown";
export { Dialog } from "./components/dialog";
export {
  DropZone,
  type DropZoneProps,
  type DropZoneState,
  type DropZoneVariant,
} from "./components/drop-zone";
export {
  EmberField,
  type EmberPalette,
  emberHue,
  type ParticleKind,
  particleFade,
  snowTopFade,
} from "./components/ember-field";
export {
  Field,
  type FieldInfo,
  Input,
  SEGMENTED_GLIDE_MS,
  SearchInput,
  Segmented,
  Select,
  type SelectOption,
  Slider,
  useField,
} from "./components/forms";
export {
  ContextMenu,
  Menu,
  type MenuItem,
  type MenuSlider,
  type MenuStripItem,
  menuHeight,
  placeBelowOrBeside,
} from "./components/menu";
export {
  PlayButton,
  type PlayButtonLabels,
  type PlayPress,
  type PlayState,
  playButtonName,
  playPress,
} from "./components/play-button";
export {
  MARK_COLORS,
  MARK_GLYPHS,
  MARK_SHAPES,
  type MarkColor,
  type MarkGlyph,
  type MarkItem,
  type MarkLook,
  type MarkShape,
  type MarkSize,
  markDrawing,
  markLookOf,
  ProfileMark,
  ProfileMarkList,
  ProfileMarks,
} from "./components/profile-mark";
export {
  SignalBars,
  type SignalState,
  type SignalTone,
  signalLevel,
  signalTone,
} from "./components/signal-bars";
export { Switch } from "./components/switch";
export { type TabItem, TabPanel, Tabs } from "./components/tabs";
export { Toaster, toast } from "./components/toaster";
export { isCut, OverflowTooltip, Tooltip, TooltipProvider } from "./components/tooltip";
export { type MarkKind, VerifiedMark } from "./components/verified-mark";
export {
  EntranceGate,
  GatedMotion,
  gateEntrance,
  prewarmImages,
  useEntranceReady,
} from "./entrance";
export {
  glideFrames,
  type OptionBox,
  PLATE_SPRING,
  type PlateDrift,
  plateAt,
  rebase,
  settleTime,
  springAt,
  underlineFrames,
} from "./glide";
export {
  focusMovedOn,
  focusPanel,
  focusTabQuietly,
  lastInputWasPointer,
  returnFocusAfterKeys,
} from "./input-modality";
export {
  keepOpenOnToast,
  type OutsideEvent,
  onToast,
  sheetOutside,
  swallowDismissClick,
} from "./layer-dismiss";
export { cn } from "./lib/cn";
export { MORE_BELOW_MASK, useMoreBelow } from "./more-below";
export * from "./motion";
export {
  fromPortal,
  guardNativeContextMenu,
  keepsNativeMenu,
  type MenuDocument,
} from "./native-menu";
export {
  PerfProvider,
  ReducedMotionConfig,
  useAmbientMotion,
  useInteractionMotion,
  useLiveMotion,
  usePerf,
  useResting,
} from "./perf";
export {
  ARM_MS,
  guardElapsed,
  guardLeave,
  guardPress,
  guardStart,
  guardSwap,
  type PressGuard,
  usePressGuard,
} from "./press-guard";
export {
  guardReducedMotion,
  motionReduced,
  setAppReducedMotion,
  stillKeyframes,
  subscribeReducedMotion,
  systemReducedMotion,
  useReduceMotion,
  useSystemReducedMotion,
} from "./reduced-motion";
export { fold, matchesQuery } from "./search";
export { observeSkippedViewTransitions } from "./view-transitions";
