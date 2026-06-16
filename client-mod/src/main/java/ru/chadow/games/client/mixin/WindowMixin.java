package ru.chadow.games.client.mixin;

import com.mojang.blaze3d.platform.Window;
import org.lwjgl.glfw.GLFW;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.BorderlessWindow;

@Mixin(Window.class)
public abstract class WindowMixin {
    @Shadow
    @Final
    private long handle;

    @Shadow
    private boolean fullscreen;

    @Shadow
    private boolean actuallyFullscreen;

    @Shadow
    private int windowedX;

    @Shadow
    private int windowedY;

    @Shadow
    private int windowedWidth;

    @Shadow
    private int windowedHeight;

    @Shadow
    private int x;

    @Shadow
    private int y;

    @Shadow
    private int width;

    @Shadow
    private int height;

    @Shadow
    private void refreshFramebufferSize() {
    }

    @Inject(method = "<init>", at = @At("TAIL"))
    private void chadow$applyBorderless(CallbackInfo ci) {
        BorderlessWindow.Spec spec = BorderlessWindow.readSpec();
        if (spec == null) {
            return;
        }

        this.fullscreen = false;
        this.actuallyFullscreen = false;
        this.windowedX = spec.x();
        this.windowedY = spec.y();
        this.windowedWidth = spec.width();
        this.windowedHeight = spec.height();
        this.x = spec.x();
        this.y = spec.y();
        this.width = spec.width();
        this.height = spec.height();

        GLFW.glfwSetWindowAttrib(this.handle, GLFW.GLFW_DECORATED, GLFW.GLFW_FALSE);
        GLFW.glfwSetWindowMonitor(this.handle, 0L, spec.x(), spec.y(), spec.width(), spec.height(), -1);
        this.refreshFramebufferSize();
    }
}
