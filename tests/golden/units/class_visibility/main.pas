// The visibility of the fields of a class declared in another unit (objects.md 2.2, 2.5): its
// `private` and `protected` fields are E0743 here, and a descendant may declare a field with the
// name of an ancestor's `private` field of another unit (hidden from it), but not with the name
// of a field it sees (E0701).
program Main;

uses Shapes;

type
  TCircle = class(TShape)
    FName: Integer;
    FSize: Integer;
    Kind: string;
  end;

var
  S: TShape;
  C: TCircle;
begin
  S := MakeShape;
  WriteLn(S.FName);
  WriteLn(S.FSize);
  WriteLn(S.FSecret);
  WriteLn(S.Kind);
  C := TCircle.Create;
  C.FName := 1;
  WriteLn(C.FName);
end.
