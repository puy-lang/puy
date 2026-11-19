program Main;

// A helper nested in a class of the `interface` section of a unit in `uses` works in the program
// (objects.md 10.4), also as `Boxes.TOuter.TH.Pick(B, …)`, and so does one nested in a class that
// a used unit names with a top-level alias (`Aliases.TRemoteAlias`; `Remote` is not used here);
// the unit's own code uses its private nested helper and one nested in a class of its
// `implementation` section.

uses Boxes, Aliases;

var
  B: TBox;
begin
  B := TBox.Create;
  B.V := 7;
  WriteLn(B.Pick(1));
  WriteLn(Boxes.TOuter.TH.Pick(B, 2));
  WriteLn(UseHidden(B));
  WriteLn(B.Far);
  B.Free;
end.
