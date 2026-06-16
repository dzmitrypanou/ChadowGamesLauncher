package ru.chadow.games.client;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.ConnectScreen;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.multiplayer.resolver.ServerAddress;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public final class PendingServerConnect {
    static final String CONNECT_FILE = "chadow-connect.txt";
    private static final int CONNECT_DELAY_TICKS = 100;

    private static String address;
    private static int ticksRemaining = -1;
    private static boolean connecting;

    private PendingServerConnect() {
    }

    public static void onClientTick(Minecraft client) {
        if (connecting) {
            return;
        }

        if (address == null) {
            readPendingAddress(client);
            return;
        }

        if (ticksRemaining > 0) {
            ticksRemaining--;
            return;
        }

        if (!isClientReady(client)) {
            return;
        }

        String target = address;
        address = null;
        ticksRemaining = -1;
        connecting = true;
        try {
            connect(client, target);
        } finally {
            connecting = false;
        }
    }

    private static void readPendingAddress(Minecraft client) {
        Path path = client.gameDirectory.toPath().resolve(CONNECT_FILE);
        if (!Files.isRegularFile(path)) {
            return;
        }

        try {
            String value = Files.readString(path).trim();
            Files.deleteIfExists(path);
            if (value.isEmpty()) {
                return;
            }
            address = value;
            ticksRemaining = CONNECT_DELAY_TICKS;
        } catch (IOException ignored) {
        }
    }

    private static boolean isClientReady(Minecraft client) {
        return client.getOverlay() == null
                && client.screen != null
                && client.player == null
                && client.level == null;
    }

    private static void connect(Minecraft client, String target) {
        ServerAddress serverAddress = ServerAddress.parseString(target);
        ServerData serverData = new ServerData(ChadowGamesClientMod.BRAND_NAME, target, ServerData.Type.OTHER);
        Screen screen = client.screen;
        if (screen == null) {
            return;
        }
        ConnectScreen.startConnecting(screen, client, serverAddress, serverData, false, null);
    }
}
