program Main;

// Where a helper nested in a class of another unit does not work (objects.md 10.4): from a private
// section of a class of another unit, and where only a descendant of the outer type
// (`TChild`) and an alias of it in a class (`TOther.TAlias`) are visible, not the outer type.

uses Boxes, Kids;

var
  B: TBox;
begin
  B := TBox.Create;
  WriteLn(B.Hid);
  WriteLn(B.Away);
  B.Free;
end.
