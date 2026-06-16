package ru.chadow.games.client.mixin;

import com.mojang.blaze3d.platform.Window;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.PauseScreen;
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

    @Shadow
    public Screen screen;

    @Shadow
    public Window window;

    @Shadow
    public abstract void setScreen(Screen screen);

    @Inject(method = "tick", at = @At("HEAD"))
    private void chadow$trackWorld(CallbackInfo ci) {
        if (this.level != null) {
            ChadowGamesClientMod.markInWorld();
        }
    }

    @Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)
    private void chadow$blockMenus(Screen screen, CallbackInfo ci) {
        if (screen == null || !ChadowGamesClientMod.shouldBlockScreen(screen)) {
            return;
        }

        ci.cancel();
        ChadowGamesClientMod.requestQuit();
        ((Minecraft) (Object) this).stop();
    }

    @Inject(method = "stop", at = @At("HEAD"), cancellable = true)
    private void chadow$guardStop(CallbackInfo ci) {
        if (ChadowGamesClientMod.consumeQuitRequest()) {
            return;
        }

        if (this.window != null && this.window.shouldClose()) {
            return;
        }

        ci.cancel();

        if (this.level != null && this.screen == null) {
            this.setScreen(new PauseScreen(false));
        }
    }

    @Inject(method = "pauseGame", at = @At("RETURN"))
    private void chadow$ensurePauseOpened(boolean pauseOnly, CallbackInfo ci) {
        if (this.level != null && this.screen == null) {
            this.setScreen(new PauseScreen(pauseOnly));
        }
    }
}
