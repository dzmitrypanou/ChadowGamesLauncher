package ru.chadow.games.client.mixin;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.PauseScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.ChadowGamesClientMod;

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

            String text = button.getMessage().getString();
            String lowered = text.toLowerCase();

            if (isTitleMenuLabel(lowered)) {
                button.visible = false;
                button.active = false;
                continue;
            }

            if (ChadowGamesClientMod.isDisconnectButtonLabel(text)) {
                ((ButtonAccessor) button).chadow$setOnPress(btn -> {
                    ChadowGamesClientMod.requestQuit();
                    minecraft.stop();
                });
            }
        }
    }

    private static boolean isTitleMenuLabel(String text) {
        return text.contains("title")
                || text.contains("titl")
                || text.contains("главн")
                || text.contains("save and quit")
                || (text.contains("сохран") && text.contains("выйти"));
    }
}
