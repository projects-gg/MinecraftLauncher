// The cloud inventory (INV-UI): `@pl/ui/inventory`. Not re-exported from `@pl/ui`: apps that do
// not show an inventory should not carry it (its font and tables).
export { type MoveMessage, messageFor } from "./controller";
export {
  SAMPLE_CHEST,
  SAMPLE_DISPENSER,
  SAMPLE_DOUBLE_CHEST,
  SAMPLE_HOPPER,
  SAMPLE_INVENTORY,
  SAMPLE_KITS,
  SAMPLE_MARKET,
  SAMPLE_SHULKER,
  SAMPLE_UNKNOWN,
  withCursor,
} from "./fixtures";
export { InventoryView, type InventoryViewProps } from "./inventory-view";
export {
  type Direction,
  neighbor,
  type Screen,
  type ScreenKind,
  type SectionId,
  type SlotKind,
  type SlotSpot,
  screenFor,
  slotKey,
  windowRows,
} from "./layout";
export {
  createMockMachine,
  type MockMachine,
  type MockMachineOptions,
  type MockWindowKind,
} from "./mock-machine";
export { glyphAdvance, LINE_BOX, textAdvance, wrapText } from "./pixel-font";
export { PixelLine, type PixelLineProps } from "./pixel-text";
export { InventoryPreview, type InventoryPreviewProps, PREVIEW_STATES } from "./preview";
export {
  bareId,
  cleanText,
  durabilityColor,
  durabilityPercent,
  durabilityWidth,
  itemName,
  itemSpeech,
  type Rarity,
  rarityOf,
  TEXT_COLOR,
  type TooltipLine,
  titleGlyphs,
  titleRuns,
  tooltipLines,
} from "./text";
export { fill, INVENTORY_TEXTS, type InventoryLang, type InventoryTexts } from "./texts";
export { layoutTitle, type TitleLayout, type TitlePicture, type TitleText } from "./title";
export type {
  InvArea,
  InvCursor,
  InvEnchant,
  InvError,
  InvErrorCode,
  InventoryMode,
  InvFrame,
  InvInsideStack,
  InvItem,
  InvLabel,
  InvOp,
  InvResult,
  SlotRef,
  TitleGlyph,
  TitleRun,
  WinFrame,
  WinOpen,
} from "./types";
