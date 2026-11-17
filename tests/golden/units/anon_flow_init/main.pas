program Main;

// Definite assignment of anonymous methods in a unit's `initialization` and `finalization`
// (objects.md 9.12): as in `<main>`, making one reads the inline variables its body may read
// before assigning them (E2000 with `captured here`).

uses AnonInit;

begin
end.
