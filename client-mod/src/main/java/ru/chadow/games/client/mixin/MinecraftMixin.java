package ru.chadow.games.client.mixin;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.ChadowGamesClientMod;

@Mixin(Minecraft.class)
public abstract class MinecraftMixin {
    @Shadow
    public ClientLevel level;

    @Inject(method = "tick", at = @At("HEAD"))
    private void chadow$trackSession(CallbackInfo ci) {
        if (this.level != null) {
            ChadowGamesClientMod.markInGame();
        }
    }

    @Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)
    private void chadow$blockMenus(Screen screen, CallbackInfo ci) {
        if (screen != null && ChadowGamesClientMod.shouldBlockScreen(screen)) {
            ci.cancel();
        }
    }
}
