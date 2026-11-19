program InlineSelfChecks;

{$Q-}
// A check of `Self` before an inlined method goes when the same variable was checked on every
// path to it and nothing since may have destroyed the instance (design/inlining.md 7.1), and
// moves before a loop that holds no call: an inlined call on `nil` or on a freed instance still
// fails at its own line, after a call or `Free` too, and a loop that runs no iteration does not
// check.

uses System.SysUtils;

type
  TBox = class
    Value: Integer;
    function Get: Integer;
    function GetThenFree: Integer;
    function Seven: Integer;
  end;

function TBox.Get: Integer;
begin
  Result := Value;
end;

function TBox.GetThenFree: Integer;
begin
  Result := Get;
  Free;
  Result := Result + Get;
end;

function TBox.Seven: Integer;
begin
  // No field access: only the check of Self catches a freed instance
  Result := 7;
end;

procedure Kill(B: TBox);
begin
  WriteLn('kill');
  B.Free;
end;

procedure Show(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
  WriteLn(E.StackTrace);
end;

procedure Twice(B: TBox);
var
  X: Integer;
begin
  X := B.Get;
  X := X + B.Get;
  WriteLn(X);
end;

procedure AfterCall(B: TBox);
var
  X: Integer;
begin
  X := B.Get;
  Kill(B);
  X := X + B.Get;
  WriteLn(X);
end;

procedure AfterFree(B: TBox);
var
  X: Integer;
begin
  X := B.Get;
  B.Free;
  X := X + B.Get;
  WriteLn(X);
end;

procedure SevenAfterFree(B: TBox);
var
  X: Integer;
begin
  X := B.Seven;
  B.Free;
  X := X + B.Seven;
  WriteLn(X);
end;

// The way into a handler counts as unchecked
procedure SevenInHandler(B: TBox);
var
  X: Integer;
begin
  X := B.Seven;
  try
    B.Free;
    raise Exception.Create('x');
  except
    X := X + B.Seven;
  end;
  WriteLn(X);
end;

procedure Loop(B: TBox; N: Integer);
var
  I, Sum: Integer;
begin
  Sum := 0;
  for I := 1 to N do
    Sum := Sum + B.Get;
  WriteLn(Sum);
end;

var
  B: TBox;
begin
  B := TBox.Create;
  B.Value := 4;
  Twice(B);
  Loop(B, 3);
  try
    Twice(nil);
  except
    on E: Exception do
      Show(E);
  end;
  Loop(nil, 0);
  try
    Loop(nil, 2);
  except
    on E: Exception do
      Show(E);
  end;
  try
    AfterCall(B);
  except
    on E: Exception do
      Show(E);
  end;
  B := TBox.Create;
  try
    AfterFree(B);
  except
    on E: Exception do
      Show(E);
  end;
  B := TBox.Create;
  try
    SevenAfterFree(B);
  except
    on E: Exception do
      Show(E);
  end;
  B := TBox.Create;
  try
    SevenInHandler(B);
  except
    on E: Exception do
      Show(E);
  end;
  B := TBox.Create;
  B.Value := 5;
  WriteLn(B.GetThenFree);
end.
