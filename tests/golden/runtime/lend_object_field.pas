program LendObjectField;

// A field of an instance passed to a `var` parameter is lent to the call (objects.md 2.2):
// touching it through another path (a global holding the same instance, `Self` in a method)
// is an `EAliasError`, as is passing it twice when that is only known at run time; other fields
// of the instance can be used. Freeing the instance during the call gives back the other
// arguments and raises `EAccessViolation` (a nil instance too). `FreeAndNil` of a field is not
// lent: it writes `nil` first, so the destructor reads `nil` there (objects.md 3.2).

uses
  System.SysUtils;

type
  TBox = class
  public
    A, B: Integer;
    Owner: TBox;
    Part: TBox;
    procedure Bump(var X: Integer);
    destructor Destroy; override;
  end;

var
  G: TBox;

procedure TBox.Bump(var X: Integer);
begin
  X := X + 1;
  try
    WriteLn('self: ', A);
  except
    on E: EAliasError do
      WriteLn('self: ', E.Message);
  end;
end;

destructor TBox.Destroy;
begin
  if Owner <> nil then
    try
      WriteLn('owner''s part: ', Assigned(Owner.Part));
    except
      on E: EAliasError do
        WriteLn('owner''s part: ', E.Message);
    end;
  inherited Destroy;
end;

procedure Touch(var X: Integer);
begin
  X := 5;
  WriteLn('other field: ', G.B);
  try
    WriteLn('read: ', G.A);
  except
    on E: EAliasError do
      WriteLn('read: ', E.Message);
  end;
  try
    G.A := 7;
  except
    on E: EAliasError do
      WriteLn('write: ', E.Message);
  end;
end;

procedure Pair(var X, Y: Integer);
begin
  X := 1;
  Y := 2;
end;

procedure FreeDuring(var X, Y: Integer);
begin
  X := 10;
  Y := 20;
  G.Free;
end;

var
  O, Other: TBox;
  I: Integer;
begin
  G := TBox.Create;
  O := G;
  Touch(O.A);
  WriteLn('after: ', O.A);
  O.Bump(O.A);
  WriteLn('after: ', O.A);
  try
    Pair(O.A, G.A);
  except
    on E: EAliasError do
      WriteLn('twice: ', E.Message);
  end;
  WriteLn('after: ', O.A);
  Other := TBox.Create;
  I := 0;
  try
    FreeDuring(Other.A, O.B);
  except
    on E: EAccessViolation do
      WriteLn('freed: ', E.Message);
  end;
  WriteLn('given back: ', Other.A);
  O := nil;
  try
    Pair(O.A, I);
  except
    on E: EAccessViolation do
      WriteLn('nil: ', E.Message);
  end;
  Other.Part := TBox.Create;
  Other.Part.Owner := Other;
  FreeAndNil(Other.Part);
  WriteLn('part: ', Assigned(Other.Part));
  Other.Free;
end.
