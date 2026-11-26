package gg.projects.overlay.tweaks.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import gg.projects.overlay.tweaks.TooltipFit;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.screens.inventory.tooltip.ClientTooltipPositioner;
import org.joml.Matrix3x2fStack;
import org.joml.Vector2i;
import org.joml.Vector2ic;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/**
 * TOOLTIP-FIT: every tooltip the game draws goes where its positioner puts it, then {@link TooltipFit#place} moves it
 * the least to show whole, and draws one larger than the screen smaller. One wrap of the positioner's call in the
 * game's single tooltip method, whatever its parameters in a version; the game's own place stands while the tweak is
 * off. Not required: a version whose method differs keeps the game's tooltips.
 */
@Mixin(GuiGraphicsExtractor.class)
abstract class GuiGraphicsExtractorMixin {
    /** This tooltip is drawn smaller: the pose this mixin pushed comes off when the tooltip is done. */
    @Unique
    private boolean projects$scaled;

    @Shadow
    public abstract Matrix3x2fStack pose();

    @WrapOperation(method = "tooltip", at = @At(value = "INVOKE",
        target = "Lnet/minecraft/client/gui/screens/inventory/tooltip/ClientTooltipPositioner;"
            + "positionTooltip(IIIIII)Lorg/joml/Vector2ic;"))
    private Vector2ic projects$fit(ClientTooltipPositioner positioner, int screenW, int screenH, int x, int y, int w,
                                  int h, Operation<Vector2ic> original) {
        Vector2ic at = original.call(positioner, screenW, screenH, x, y, w, h);
        TooltipFit.Place place = TooltipFit.place(at.x(), at.y(), w, h, screenW, screenH);
        if (place == null) {
            return at;
        }
        if (place.scale() < 1F) {
            Matrix3x2fStack pose = pose();
            pose.pushMatrix();
            pose.translate(place.x(), place.y());
            pose.scale(place.scale());
            projects$scaled = true;
            return new Vector2i(0, 0);
        }
        return place.x() == at.x() && place.y() == at.y() ? at : new Vector2i(place.x(), place.y());
    }

    @Inject(method = "tooltip", at = @At("TAIL"))
    private void projects$unscale(CallbackInfo info) {
        if (projects$scaled) {
            projects$scaled = false;
            pose().popMatrix();
        }
    }
}
