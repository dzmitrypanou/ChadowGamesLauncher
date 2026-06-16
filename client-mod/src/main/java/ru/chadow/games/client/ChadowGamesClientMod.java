package ru.chadow.games.client;

import net.fabricmc.api.ClientModInitializer;

public final class ChadowGamesClientMod implements ClientModInitializer {
    public static final String BRAND_NAME = "Chadow Games Launcher";

    private static boolean inGameSession;

    @Override
    public void onInitializeClient() {
        inGameSession = false;
    }

    public static void markInGame() {
        inGameSession = true;
    }

    public static boolean isInGameSession() {
        return inGameSession;
    }

    public static boolean shouldBlockScreen(net.minecraft.client.gui.screens.Screen screen) {
        if (screen instanceof net.minecraft.client.gui.screens.PauseScreen) {
            return true;
        }
        if (screen instanceof net.minecraft.client.gui.screens.TitleScreen && inGameSession) {
            return true;
        }
        if (screen instanceof net.minecraft.client.gui.screens.DisconnectedScreen) {
            return false;
        }
        return false;
    }
}
