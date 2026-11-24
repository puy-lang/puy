program UnavailableNamesWithGenerics;

// With System.Generics.Collections, `TList` names the generic TList<T> and is E0761 without type
// arguments; once System.Contnrs is used too, it is E3022 (requirements §8.6), also when
// qualified with the unit. TList<T> itself and the containers of System.Contnrs are not
// affected.

uses
  System.Generics.Collections, System.Contnrs;

var
  A: TList;
  B: System.Contnrs.TQueue;
  C: TList<Integer>;
  D: TObjectList;
  E: TStack;

begin
end.
