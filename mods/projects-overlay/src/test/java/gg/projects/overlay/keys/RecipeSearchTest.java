package gg.projects.overlay.keys;

import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class RecipeSearchTest {
    @Test
    void theRecipeBooksSearchBoxIsFoundOnThisVersion() {
        // The book keeps its search box to itself: a version that renamed the field would leave the guard blind there.
        assertTrue(RecipeSearch.known());
    }
}
