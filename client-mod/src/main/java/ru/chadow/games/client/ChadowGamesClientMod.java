package ru.chadow.games.client;

import com.mojang.realmsclient.RealmsMainScreen;
import net.fabricmc.api.ClientModInitializer;
import net.minecraft.ChatFormatting;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.client.gui.screens.multiplayer.JoinMultiplayerScreen;

import java.util.Locale;

public final class ChadowGamesClientMod implements ClientModInitializer {
    public static final String MOD_ID = "chadow_games_client";
    public static final String BRAND_NAME = "Chadow Games";

    private static boolean quitRequested;
    private static boolean beenInWorld;

    @Override
    public void onInitializeClient() {
        quitRequested = false;
        beenInWorld = false;
    }

    public static void markInWorld() {
        beenInWorld = true;
    }

    public static void requestQuit() {
        quitRequested = true;
    }

    public static boolean consumeQuitRequest() {
        boolean requested = quitRequested;
        quitRequested = false;
        return requested;
    }

    public static boolean shouldBlockScreen(net.minecraft.client.gui.screens.Screen screen) {
        if (!beenInWorld) {
            return false;
        }

        return screen instanceof TitleScreen
                || screen instanceof JoinMultiplayerScreen
                || screen instanceof RealmsMainScreen;
    }

    public static String normalizeLabel(String text) {
        return ChatFormatting.stripFormatting(text).toLowerCase(Locale.ROOT).trim();
    }

    public static boolean isPauseUtilityButton(String text) {
        String lowered = normalizeLabel(text);
        return lowered.contains("return to game")
                || lowered.contains("back to game")
                || lowered.contains("вернуться")
                || lowered.contains("продолж")
                || lowered.contains("options")
                || lowered.contains("настрой")
                || lowered.contains("advance")
                || lowered.contains("прогресс")
                || lowered.contains("feedback")
                || lowered.contains("open to lan")
                || lowered.contains("открыть для сети");
    }

    public static boolean isDisconnectButtonLabel(String text) {
        if (isPauseUtilityButton(text) || isReportButtonLabel(text)) {
            return false;
        }

        String lowered = normalizeLabel(text);
        return lowered.contains("disconnect")
                || lowered.contains("отключ")
                || lowered.contains("покинуть")
                || lowered.contains("leave server")
                || lowered.contains("выйти с сервера");
    }

    public static boolean isLobbyNavigationLabel(String text) {
        String lowered = normalizeLabel(text);
        return lowered.contains("title")
                || lowered.contains("titl")
                || lowered.contains("главн")
                || lowered.contains("menu")
                || lowered.contains("меню")
                || lowered.contains("список")
                || lowered.contains("server list")
                || lowered.contains("to menu")
                || lowered.contains("к сервер")
                || lowered.contains("back to server");
    }

    public static boolean isReportButtonLabel(String text) {
        String lowered = normalizeLabel(text);
        return lowered.contains("report")
                || lowered.contains("отчёт")
                || lowered.contains("отчет")
                || lowered.contains("bug")
                || lowered.contains("папк");
    }
}
