program ForInEnumCleanup;

uses System.SysUtils;

// The `finally` of `for..in` with `GetEnumerator` (objects.md 13.2): the enumerator managed by
// hand is destroyed when the loop ends by `Break`, `Continue` to the end, `Exit` and an exception
// from the body; `GetEnumerator` returning `nil` raises `EAccessViolation` at the first
// `MoveNext` (and the cleanup's `Free` does nothing), and so does a `nil` collection at
// `GetEnumerator`.

type
  TEnum = class
    I, N: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TColl = class
    NilEnum: Boolean;
    function GetEnumerator: TEnum;
  end;

destructor TEnum.Destroy;
begin
  WriteLn('  destroyed at ', I);
  inherited;
end;

function TEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= N;
end;

function TEnum.GetCurrent: Integer;
begin
  Result := I;
end;

function TColl.GetEnumerator: TEnum;
begin
  if NilEnum then
    Exit(nil);
  Result := TEnum.Create;
  Result.N := 3;
end;

procedure WithBreak(C: TColl);
begin
  WriteLn('break');
  for var X in C do
    if X = 2 then
      Break;
  WriteLn('  after the loop');
end;

procedure WithContinue(C: TColl);
begin
  WriteLn('continue');
  for var X in C do
  begin
    if X > 1 then
      Continue;
    WriteLn('  ', X);
  end;
end;

procedure WithExit(C: TColl);
begin
  WriteLn('exit');
  for var X in C do
    if X = 2 then
      Exit;
  WriteLn('  not reached');
end;

procedure WithRaise(C: TColl);
begin
  WriteLn('raise');
  for var X in C do
    if X = 2 then
      raise Exception.Create('from the body at ' + IntToStr(X));
end;

procedure Iterate(C: TColl);
begin
  for var X in C do
    WriteLn('  ', X);
end;

var
  C: TColl;
begin
  C := TColl.Create;
  WithBreak(C);
  WithContinue(C);
  WithExit(C);
  try
    WithRaise(C);
  except
    on E: Exception do
      WriteLn('  ', E.Message);
  end;
  C.NilEnum := True;
  try
    Iterate(C);
  except
    on E: EAccessViolation do
      WriteLn('nil enumerator: ', E.Message);
  end;
  C.Free;
  C := nil;
  try
    Iterate(C);
  except
    on E: EAccessViolation do
      WriteLn('nil collection: ', E.Message);
  end;
end.
