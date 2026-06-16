package ru.chadow.games.client.mixin;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.ChadowGamesClientMod;

@Mixin(DisconnectedScreen.class)
public abstract class DisconnectedScreenMixin {
    @Inject(method = "init", at = @At("RETURN"))
    private void chadow$quitInsteadOfLobby(CallbackInfo ci) {
        DisconnectedScreen screen = (DisconnectedScreen) (Object) this;
        Minecraft minecraft = Minecraft.getInstance();

        for (var child : screen.children()) {
            if (!(child instanceof Button button)) {
                continue;
            }

            String text = button.getMessage().getString();
            if (ChadowGamesClientMod.isReportButtonLabel(text)) {
                continue;
            }
            if (!ChadowGamesClientMod.isLobbyNavigationLabel(text)) {
                continue;
            }

            ((ButtonAccessor) button).chadow$setOnPress(btn -> {
                ChadowGamesClientMod.requestQuit();
                minecraft.stop();
            });
        }
    }
}
