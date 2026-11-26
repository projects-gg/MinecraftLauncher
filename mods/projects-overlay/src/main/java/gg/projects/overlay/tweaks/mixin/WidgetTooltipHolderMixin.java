package gg.projects.overlay.tweaks.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import com.llamalad7.mixinextras.sugar.Local;
import gg.projects.overlay.tweaks.TooltipFit;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.components.WidgetTooltipHolder;
import net.minecraft.client.gui.navigation.ScreenRectangle;
import net.minecraft.util.FormattedCharSequence;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/**
 * TOOLTIP-FIT: a widget's tooltip (a button's, a server window's) gets its lines from {@link TooltipFit#lines}: the
 * game's own while they fit beside the widget, else its text wrapped wider so it does. Not required: a version whose
 * holder differs keeps the game's lines (the tooltip still moves onto the screen).
 */
@Mixin(WidgetTooltipHolder.class)
abstract class WidgetTooltipHolderMixin {
    @WrapOperation(method = "refreshTooltipForNextRenderPass", at = @At(value = "INVOKE",
        target = "Lnet/minecraft/client/gui/components/Tooltip;toCharSequence(Lnet/minecraft/client/Minecraft;)"
            + "Ljava/util/List;"))
    private List<FormattedCharSequence> projects$wrap(Tooltip tooltip, Minecraft minecraft,
                                                      Operation<List<FormattedCharSequence>> original,
                                                      @Local(argsOnly = true) GuiGraphicsExtractor graphics,
                                                      @Local(argsOnly = true) ScreenRectangle widget) {
        List<FormattedCharSequence> lines = original.call(tooltip, minecraft);
        if (!TooltipFit.on()) {
            return lines;
        }
        return TooltipFit.lines(tooltip, lines, minecraft.font, graphics.guiWidth(), graphics.guiHeight(), widget);
    }
}
