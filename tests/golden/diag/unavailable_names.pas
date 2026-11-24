program UnavailableNames;

// A name of a standard library unit that Puy does not provide (requirements §8.6) is E3022 in
// place of E0700, whether or not the unit is used: in a type, an expression and a parent class.
// The message names the unit and the name as the unit's list writes it (`tstack` is reported as
// `TStack`), and the note says what to use instead. A name written with type arguments
// (`TList<Integer>` without System.Generics.Collections) stays E0700.

type
  TMyList = class(TList)
  end;

var
  L: TList;
  S: tstack;
  B: TBucketList;
  C: TComponentList;
  G: TList<Integer>;

begin
  L := TList.Create;
end.
