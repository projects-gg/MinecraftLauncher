package gg.projects.overlay.tweaks;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import gg.projects.overlay.tweaks.SecretLabel.Kind;
import gg.projects.overlay.tweaks.SecretLabel.Node;
import java.util.List;
import org.junit.jupiter.api.Test;

/** PASSWORD-EYE's rules: which words name a password, and which boxes of a server window are password boxes. */
class SecretLabelTest {
    private static Node text(String text) {
        return new Node(Kind.TEXT, text);
    }

    private static Node box(String label) {
        return new Node(Kind.BOX, label);
    }

    private static Node other() {
        return new Node(Kind.OTHER, "");
    }

    @Test
    void passwordWordsAreFoundHoweverTheyAreWritten() {
        for (String label : List.of("Şifre:", "ŞİFRE", "şifreniz", "Sifre", "SIFRE", "Şifre Tekrar", "Parola",
            "parolanızı yazınız", "PAROLA", "Password", "Confirm password", "Passwort", "Contraseña", "Senha",
            "Mot de passe", "Hasło", "Пароль", "\u00a7eŞifre\u00a7r:", "Yeni şifreni gir")) {
            assertTrue(SecretLabel.names(label), label);
        }
        for (String label : List.of("Ad", "E-posta", "Kullanıcı Adı", "Klan Etiketi", "Battle Pass", "Kod", "", " ")) {
            assertFalse(SecretLabel.names(label), label);
        }
        assertFalse(SecretLabel.names(null));
    }

    @Test
    void foldingReadsTurkishLettersAsPlayersTypeThem() {
        assertEquals("sifre", SecretLabel.fold("ŞİFRE"));
        assertEquals("sifre", SecretLabel.fold("Şifre"));
        assertEquals("parolanizi", SecretLabel.fold("parolanızı"));
        assertEquals("sifre:", SecretLabel.fold("\u00a76Şifre\u00a7r:"));
    }

    @Test
    void theServersLoginWindowHidesItsPasswordBox() {
        // The window from the owner's screenshot: a text, the labelled box, a check box.
        List<Node> window = List.of(text("Giriş"), text("Merhaba Kaan_07! Lütfen aşağıya parolanızı yazınız:"),
            text("Şifre:"), box("Şifre:"), other());
        assertArrayEquals(new boolean[] {false, false, false, true, false}, SecretLabel.secret(window));
    }

    @Test
    void aBoxWhoseLabelIsHiddenTakesTheTextRightAboveIt() {
        List<Node> window = List.of(text("Lütfen parolanı yaz:"), box(" "));
        assertArrayEquals(new boolean[] {false, true}, SecretLabel.secret(window));
        // The hidden label names it already.
        assertArrayEquals(new boolean[] {false, true}, SecretLabel.secret(List.of(text("Giriş"), box("Parola"))));
    }

    @Test
    void aShownLabelOfAnotherKindKeepsTheBoxClearWhateverTheWindowSaysAbove() {
        List<Node> window = List.of(text("Şifreni unuttuysan e-postanı yaz:"), text("E-posta"), box("E-posta"));
        assertArrayEquals(new boolean[] {false, false, false}, SecretLabel.secret(window));
    }

    @Test
    void onlyTheBoxRightBelowAPasswordTextTakesIt() {
        // A registration form: name, then password twice; a check box between breaks the reading.
        List<Node> window = List.of(text("Kayıt Ol"), text("Ad"), box("Ad"), text("Şifre"), box("Şifre"),
            text("Şifre Tekrar"), box("Şifre Tekrar"), text("Lütfen şifreni yaz"), other(), box(""));
        assertArrayEquals(new boolean[] {false, false, false, false, true, false, true, false, false, false},
            SecretLabel.secret(window));
    }

    @Test
    void aWindowWithoutPasswordsHasNone() {
        List<Node> window = List.of(text("Klan Kur"), text("Klanına bir ad ver."), other(), text("Ad"), box("Ad"),
            text("Etiket"), box("Etiket"));
        for (boolean secret : SecretLabel.secret(window)) {
            assertFalse(secret);
        }
    }
}
