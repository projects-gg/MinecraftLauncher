package gg.projects.overlay.tweaks.mixin;

import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** TOOLTIP-FIT reads a widget tooltip's text to wrap it wider ({@link gg.projects.overlay.tweaks.TooltipFit#lines}). */
@Mixin(Tooltip.class)
public interface TooltipText {
    @Accessor("message")
    Component projects$message();
}
