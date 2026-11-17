program NestedValues;

uses System.SysUtils;

// Nested routines made values of anonymous method types (objects.md 9.7): they capture the
// variables they use like anonymous methods and can be called after the routine they are nested
// in returns, including mutually recursive ones and ones calling the other nested routines
// they reach. Each call of the enclosing routine has its own variables.

type
  TCounter = reference to function: Integer;
  TTest = reference to function(N: Integer): Boolean;

function MakeCounter(Start: Integer): TCounter;
var
  N: Integer;

  function Next: Integer;
  begin
    Inc(N);
    Result := N;
  end;

begin
  N := Start;
  Result := Next;
end;

function Parity(Even: Boolean): TTest;
  function IsOdd(X: Integer): Boolean; forward;

  function IsEven(X: Integer): Boolean;
  begin
    Result := (X = 0) or IsOdd(X - 1);
  end;

  function IsOdd(X: Integer): Boolean;
  begin
    Result := (X <> 0) and IsEven(X - 1);
  end;

begin
  if Even then
    Result := IsEven
  else
    Result := IsOdd;
end;

function Report(const Title: string; Limit: Integer): TProc;
var
  Lines: Integer;

  procedure Line(const S: string);
  begin
    Inc(Lines);
    WriteLn(Lines, ': ', S);
  end;

  // Calls `Line` (another group) and recurses into itself
  procedure Countdown(K: Integer);
  begin
    if K < 0 then
      Exit;
    Line(Title + ' ' + IntToStr(K));
    Countdown(K - 1);
  end;

  procedure Run;
    // A routine nested in a closure-form routine, calling its group and another one
    procedure Twice;
    begin
      Countdown(1);
      Line('again');
    end;
  begin
    Countdown(Limit);
    Twice;
  end;

begin
  Lines := 0;
  Line('start');
  Result := Run;
end;

var
  A, B: TCounter;
  T: TTest;
  P: TProc;
  I: Integer;
begin
  A := MakeCounter(10);
  B := MakeCounter(100);
  WriteLn(A(), ' ', A(), ' ', B(), ' ', A());
  T := Parity(True);
  for I := 0 to 5 do
    Write(T(I), ' ');
  WriteLn;
  T := Parity(False);
  WriteLn(T(7), ' ', T(8));
  P := Report('down', 2);
  P();
  P();
end.
