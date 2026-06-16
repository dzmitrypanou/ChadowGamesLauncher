package ru.chadow.games.client.mixin;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.PauseScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(PauseScreen.class)
public abstract class PauseScreenMixin {
    @Inject(method = "createPauseMenu", at = @At("RETURN"))
    private void chadow$customizePauseMenu(CallbackInfo ci) {
        PauseScreen screen = (PauseScreen) (Object) this;
        Minecraft minecraft = Minecraft.getInstance();

        for (var child : screen.children()) {
            if (!(child instanceof Button button)) {
                continue;
            }

            String text = button.getMessage().getString().toLowerCase();
            if (isDisconnectLabel(text)) {
                ((ButtonAccessor) button).chadow$setOnPress(pressed -> minecraft.stop());
                continue;
            }

            if (isTitleMenuLabel(text)) {
                button.visible = false;
                button.active = false;
            }
        }
    }

    private static boolean isDisconnectLabel(String text) {
        return text.contains("disconnect")
                || text.contains("отключ")
                || text.contains("отсоедин");
    }

    private static boolean isTitleMenuLabel(String text) {
        return text.contains("title")
                || text.contains("titl")
                || text.contains("главн")
                || text.contains("save and quit")
                || (text.contains("сохран") && text.contains("выйти"));
    }
}
