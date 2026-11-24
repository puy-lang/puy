program UnavailableNamesGenericOnly;

// Without System.Contnrs, a generic type of the same name keeps E0761 (`docs/spec/generics.md`
// 2.3): the name not provided by System.Contnrs is E3022 only when that unit is used.

uses
  System.Generics.Collections;

var
  A: TList;

begin
end.
