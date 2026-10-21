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
export { Countdown, countdownCells, countdownTickMs } from "./components/countdown";
export { Dialog } from "./components/dialog";
export {
  DropZone,
  type DropZoneProps,
  type DropZoneState,
  type DropZoneVariant,
} from "./components/drop-zone";
export { EmberField, type ParticleKind } from "./components/ember-field";
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
export { ContextMenu, Menu, type MenuItem } from "./components/menu";
export {
  PlayButton,
  type PlayButtonLabels,
  type PlayPress,
  type PlayState,
  playButtonName,
  playPress,
} from "./components/play-button";
export { SignalBars, type SignalState, signalLevel } from "./components/signal-bars";
export { Switch } from "./components/switch";
export { type TabItem, TabPanel, Tabs } from "./components/tabs";
export { Toaster, toast } from "./components/toaster";
export { isCut, OverflowTooltip, Tooltip, TooltipProvider } from "./components/tooltip";
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
  focusPanel,
  focusTabQuietly,
  lastInputWasPointer,
  returnFocusAfterKeys,
} from "./input-modality";
export { cn } from "./lib/cn";
export * from "./motion";
export {
  fromPortal,
  guardNativeContextMenu,
  keepsNativeMenu,
  type MenuDocument,
} from "./native-menu";
export {
  PerfProvider,
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
export { guardReducedMotion, stillKeyframes } from "./reduced-motion";
export { fold, matchesQuery } from "./search";
export { observeSkippedViewTransitions } from "./view-transitions";
