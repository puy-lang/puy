program HelperMethodRefFreed;

// A class helper's method made a method reference holds the instance weakly and checks it when
// called (objects.md 10.8, 9.3): it works while the instance lives and raises
// `EAccessViolation` after `Free`.

type
  TFoo = class
  public
    Name: string;
  end;

  TFooHelper = class helper for TFoo
    procedure Hi;
  end;

procedure TFooHelper.Hi;
begin
  WriteLn('hi from ', Name);
end;

var
  F: TFoo;
  P: procedure of object;
begin
  F := TFoo.Create;
  F.Name := 'foo';
  P := F.Hi;
  P;
  F.Free;
  P;
  WriteLn('not reached');
end.
