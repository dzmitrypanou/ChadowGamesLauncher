package ru.chadow.games.client.mixin;

import net.minecraft.client.DeltaTracker;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GuiGraphics;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import ru.chadow.games.client.ChadowGamesClientMod;

@Mixin(Gui.class)
public abstract class InGameHudMixin {
    private static final Identifier LOGO_TEXTURE = Identifier.fromNamespaceAndPath(
            ChadowGamesClientMod.MOD_ID,
            "textures/gui/logo.png"
    );

    @Shadow
    @Final
    private Minecraft minecraft;

    @Inject(method = "render", at = @At("RETURN"))
    private void chadow$renderBrand(GuiGraphics graphics, DeltaTracker tick, CallbackInfo ci) {
        if (this.minecraft.level == null || this.minecraft.screen != null) {
            return;
        }

        String title = ChadowGamesClientMod.BRAND_NAME;
        int padding = 8;
        int logoSize = 12;
        int gap = 6;
        int textWidth = this.minecraft.font.width(title);
        int barWidth = padding + logoSize + gap + textWidth + padding;
        int barHeight = 16;
        int x = (graphics.guiWidth() - barWidth) / 2;
        int y = 4;

        graphics.fill(x, y, x + barWidth, y + barHeight, 0xCC0A1022);
        graphics.fill(x, y, x + barWidth, y + 1, 0xFF36E0FF);
        graphics.blit(LOGO_TEXTURE, x + padding, y + 2, logoSize, logoSize, 0.0F, 0.0F, 1.0F, 1.0F);
        graphics.drawString(this.minecraft.font, title, x + padding + logoSize + gap, y + 4, 0xFFE8F2FF, true);
    }
}
