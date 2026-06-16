package ru.chadow.games.client.mixin;

import net.minecraft.client.gui.components.Button;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(Button.class)
public interface ButtonAccessor {
    @Accessor("onPress")
    @Mutable
    void chadow$setOnPress(Button.OnPress onPress);
}
