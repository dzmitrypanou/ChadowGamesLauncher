package ru.chadow.games.client;

import net.fabricmc.loader.api.FabricLoader;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;

public final class BorderlessWindow {
    public static final String DISPLAY_FILE = "chadow-display.txt";

    private static Spec activeSpec;

    private BorderlessWindow() {
    }

    public enum Mode {
        BORDERLESS,
        WINDOWED
    }

    public record Spec(Mode mode, int x, int y, int width, int height) {
    }

    public static Spec readSpec() {
        if (activeSpec != null) {
            return activeSpec;
        }

        Path path = FabricLoader.getInstance().getGameDir().resolve(DISPLAY_FILE);
        if (!Files.isRegularFile(path)) {
            return null;
        }

        try {
            String[] parts = Files.readString(path).trim().split("\\s+");
            Files.deleteIfExists(path);
            if (parts.length != 5) {
                return null;
            }

            Mode mode = switch (parts[0].toLowerCase()) {
                case "borderless" -> Mode.BORDERLESS;
                case "windowed" -> Mode.WINDOWED;
                default -> null;
            };
            if (mode == null) {
                return null;
            }

            activeSpec = new Spec(
                    mode,
                    Integer.parseInt(parts[1]),
                    Integer.parseInt(parts[2]),
                    Math.max(Integer.parseInt(parts[3]), 1),
                    Math.max(Integer.parseInt(parts[4]), 1)
            );
            return activeSpec;
        } catch (IOException | NumberFormatException ignored) {
            return null;
        }
    }
}
