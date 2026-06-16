package ru.chadow.games.client.mixin;

import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.DisconnectedScreen;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(DisconnectedScreen.class)
public abstract class DisconnectedScreenMixin {
    @Inject(method = "init", at = @At("RETURN"))
    private void chadow$hideTitleButton(CallbackInfo ci) {
        DisconnectedScreen screen = (DisconnectedScreen) (Object) this;
        for (var child : screen.children()) {
            if (!(child instanceof Button button)) {
                continue;
            }
            Component message = button.getMessage();
            String text = message.getString().toLowerCase();
            if (text.contains("title") || text.contains("главн") || text.contains("titl")) {
                button.visible = false;
                button.active = false;
            }
        }
    }
}
