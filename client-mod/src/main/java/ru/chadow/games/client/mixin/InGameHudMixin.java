package ru.chadow.games.client.mixin;

import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GuiGraphics;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.ChadowGamesClientMod;

@Mixin(Gui.class)
public abstract class InGameHudMixin {
    @Shadow
    @Final
    private Minecraft minecraft;

    @Shadow
    @Final
    private Font font;

    @Inject(method = "render", at = @At("RETURN"))
    private void chadow$renderBrand(GuiGraphics graphics, DeltaTracker tick, CallbackInfo ci) {
        if (this.minecraft.level == null || this.minecraft.screen != null) {
            return;
        }

        String title = ChadowGamesClientMod.BRAND_NAME;
        int padding = 8;
        int textWidth = this.font.width(title);
        int barWidth = textWidth + padding * 2;
        int barHeight = 16;
        int x = (graphics.guiWidth() - barWidth) / 2;
        int y = 4;

        graphics.fill(x, y, x + barWidth, y + barHeight, 0xCC0A1022);
        graphics.fill(x, y, x + barWidth, y + 1, 0xFF36E0FF);
        graphics.drawString(this.font, title, x + padding, y + 4, 0xFFE8F2FF, true);
    }
}
