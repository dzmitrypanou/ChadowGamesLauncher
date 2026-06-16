package ru.chadow.games.client;

import com.mojang.realmsclient.RealmsMainScreen;
import net.fabricmc.api.ClientModInitializer;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;

import java.util.Locale;

public final class ChadowGamesClientMod implements ClientModInitializer {
    public static final String MOD_ID = "chadow_games_client";
    public static final String BRAND_NAME = "Chadow Games";

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
        if (!inGameSession) {
            return false;
        }

        return screen instanceof TitleScreen
                || screen instanceof JoinMultiplayerScreen
                || screen instanceof RealmsMainScreen;
    }

    public static boolean isLobbyNavigationLabel(String text) {
        String lowered = text.toLowerCase(Locale.ROOT);
        return lowered.contains("title")
                || lowered.contains("titl")
                || lowered.contains("главн")
                || lowered.contains("menu")
                || lowered.contains("меню")
                || lowered.contains("список")
                || lowered.contains("server list")
                || lowered.contains("to menu")
                || lowered.contains("к сервер");
    }

    public static boolean isReportButtonLabel(String text) {
        String lowered = text.toLowerCase(Locale.ROOT);
        return lowered.contains("report")
                || lowered.contains("отчёт")
                || lowered.contains("отчет")
                || lowered.contains("bug")
                || lowered.contains("папк");
    }
}
