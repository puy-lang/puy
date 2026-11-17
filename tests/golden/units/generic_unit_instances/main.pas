program Main;

uses Boxes;

var
  A: TBox<Integer>;
  B: Boxes.TBox<string>;
begin
  A := IntBox(7);
  WriteLn(A.Get);
  WriteLn(A is TBox<Integer>);
  A.Free;
  B := Boxes.TBox<string>.Create('s');
  WriteLn(B.Get);
  B.Free;
end.
