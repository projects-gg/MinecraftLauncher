// Turkish-aware search folding lives in the shared package (Studio uses it too); this keeps the
// launcher's imports short and gives its tests (search.test.ts) one place to reach it.
export { fold, matchesQuery } from "@pl/ui/search";
