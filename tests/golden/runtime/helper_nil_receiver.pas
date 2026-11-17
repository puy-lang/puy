program HelperNilReceiver;

// A class helper's method checks its receiver when called (objects.md 10.8): calling it on
// `nil` raises `EAccessViolation`, as a method of the class itself does.

type
  TFoo = class
  end;

  TFooHelper = class helper for TFoo
    procedure Hi;
  end;

procedure TFooHelper.Hi;
begin
  WriteLn('hi');
end;

var
  F: TFoo;
begin
  F := TFoo.Create;
  F.Hi;
  F.Free;
  F := nil;
  F.Hi;
  WriteLn('not reached');
end.
