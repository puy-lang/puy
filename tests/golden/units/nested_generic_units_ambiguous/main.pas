program Main;

// The class before a nested generic type named without the unit (`TC.TG<Integer>`) is looked
// up in the units of `uses` as an unqualified name: declared in two of them, it is E2502 in a
// type position and in an expression (generics.md 2.5, units.md 3.1).

uses Left, Right;

var
  K: TC.TG<Integer>;
  O: TObject;
begin
  O := TC.TG<Integer>.Create;
  WriteLn(O is TC.TG<Integer>);
  O := Left.TC.TG<Integer>.Create;
end.
