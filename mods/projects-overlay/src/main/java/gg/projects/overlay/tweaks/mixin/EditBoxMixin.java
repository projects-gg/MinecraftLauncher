package gg.projects.overlay.tweaks.mixin;

import com.llamalad7.mixinextras.injector.ModifyExpressionValue;
import com.llamalad7.mixinextras.injector.ModifyReturnValue;
import gg.projects.overlay.tweaks.PasswordEye;
import gg.projects.overlay.tweaks.SecretBox;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.client.input.PreeditEvent;
import org.objectweb.asm.Opcodes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/**
 * PASSWORD-EYE on the game's text box. A box {@link PasswordEye} marked keeps its real text (the window sends it as
 * typed) but draws and measures dots of the same count while hidden: every place the box reads its text to draw it,
 * place the caret, scroll, find a clicked character or a word gets the dots instead. So the caret, the selection and
 * clicks sit on the dots, and words are not told apart (no spaces to see). While hidden, copying and cutting do
 * nothing; the narrator never reads a password box. Its text area ends before the eye; a press on the eye shows or
 * hides the text and moves no caret. Boxes nobody marked, and every box while the tweak is off, are the game's own.
 * Not required: an injection a version does not take leaves that part as the game has it.
 */
@Mixin(EditBox.class)
abstract class EditBoxMixin implements SecretBox {
    @Unique
    private boolean projects$secret;
    @Unique
    private boolean projects$shown;
    /** The press under way began on the eye: dragging from it selects nothing. */
    @Unique
    private boolean projects$eyePress;
    /** The text the dots were last made for, and the dots. */
    @Unique
    private String projects$dotsOf;
    @Unique
    private String projects$dots = "";

    @Unique
    private EditBox projects$self() {
        return (EditBox) (Object) this;
    }

    @Override
    public void projects$secret(boolean secret) {
        if (projects$secret != secret) {
            projects$secret = secret;
            projects$shown = false;
            projects$refresh();
        }
    }

    @Override
    public boolean projects$isSecret() {
        return projects$secret && PasswordEye.on();
    }

    @Override
    public boolean projects$hidden() {
        return projects$secret && !projects$shown && PasswordEye.on();
    }

    /** The text's width or look changed: the box scrolls and places its text again, keeping caret and selection. */
    @Unique
    private void projects$refresh() {
        EditBox box = projects$self();
        box.moveCursorTo(box.getCursorPosition(), true);
    }

    @ModifyExpressionValue(method = {"extractWidgetRenderState", "findClickedPositionInText", "updateTextPosition",
        "scrollTo", "getScreenX", "getWordPosition(IIZ)I"}, at = @At(value = "FIELD",
        target = "Lnet/minecraft/client/gui/components/EditBox;value:Ljava/lang/String;", opcode = Opcodes.GETFIELD))
    private String projects$drawn(String value) {
        if (!projects$hidden()) {
            return value;
        }
        if (value != projects$dotsOf) {
            projects$dotsOf = value;
            projects$dots = PasswordEye.mask(value);
        }
        return projects$dots;
    }

    @ModifyExpressionValue(method = "createNarrationMessage", at = @At(value = "FIELD",
        target = "Lnet/minecraft/client/gui/components/EditBox;value:Ljava/lang/String;", opcode = Opcodes.GETFIELD))
    private String projects$narrated(String value) {
        return projects$isSecret() ? "" : value;
    }

    @Inject(method = "keyPressed", at = @At("HEAD"), cancellable = true)
    private void projects$noCopy(KeyEvent event, CallbackInfoReturnable<Boolean> result) {
        if (projects$hidden() && (event.isCopy() || event.isCut())) {
            result.setReturnValue(true);
        }
    }

    /** While hidden, text being composed with an input method is not drawn in clear either. */
    @Inject(method = "preeditUpdated", at = @At("HEAD"), cancellable = true)
    private void projects$noPreedit(PreeditEvent event, CallbackInfoReturnable<Boolean> result) {
        if (projects$hidden()) {
            result.setReturnValue(true);
        }
    }

    @ModifyReturnValue(method = "getInnerWidth", at = @At("RETURN"))
    private int projects$roomForEye(int width) {
        return projects$isSecret() ? Math.max(0, width - PasswordEye.SPACE) : width;
    }

    @Inject(method = "extractWidgetRenderState", at = @At("TAIL"))
    private void projects$eye(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float a, CallbackInfo info) {
        if (projects$isSecret() && projects$self().isVisible()) {
            PasswordEye.drawEye(graphics, projects$self(), projects$shown, mouseX, mouseY);
        }
    }

    @Inject(method = "onClick", at = @At("HEAD"), cancellable = true)
    private void projects$eyeClick(MouseButtonEvent event, boolean doubleClick, CallbackInfo info) {
        projects$eyePress = projects$isSecret() && PasswordEye.onEye(projects$self(), event.x(), event.y());
        if (projects$eyePress) {
            projects$shown = !projects$shown;
            projects$refresh();
            PasswordEye.pressed();
            info.cancel();
        }
    }

    @Inject(method = "onDrag", at = @At("HEAD"), cancellable = true)
    private void projects$eyeDrag(MouseButtonEvent event, double dx, double dy, CallbackInfo info) {
        if (projects$eyePress) {
            info.cancel();
        }
    }
}
