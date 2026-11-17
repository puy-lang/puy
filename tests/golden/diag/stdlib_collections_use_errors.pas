// Mistakes in the type arguments and arguments given to the types of the standard library are
// reported at the line of the user's code, not inside the unit of the standard library
// (docs/spec/collections.md 1.2).
program StdlibCollectionsUseErrors;

uses System.Generics.Collections, System.Generics.Defaults;

var
  P: TPair<Integer>;
  Q: TPair<string, Integer>;
  E: TEnumerable<Integer, string>;
begin
  Q := TPair<string, Integer>.Create(1, 2);
end.
