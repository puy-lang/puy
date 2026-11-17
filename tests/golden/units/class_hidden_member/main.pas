// A member the class cannot see is skipped by the name lookup in a method (objects.md 2.3, 2.5):
// in a descendant declared in another unit, the ancestor's `private` field `Secret` is hidden, so
// `Secret` names the global; the `strict protected` field `Size` is seen from the descendant's
// methods.
program Main;

uses Shapes;

type
  TCircle = class(TShape)
    procedure Show;
  end;

var
  Secret: string;

procedure TCircle.Show;
begin
  WriteLn(Secret, ' ', Size);
end;

var
  C: TCircle;
begin
  Secret := 'global';
  C := TCircle.Create;
  C.Show;
  C.Free;
end.
