program Nested;

// Nested routines read and write the parameters, local variables and Result of the routines
// enclosing them, two or more levels out (semantics.md 4.9)

type
  TPoint = record
    X, Y: Integer;
  end;

const
  Start: TPoint = (X: 0; Y: 5);

function Outer(N: Integer; const Prefix: string): string;
var
  Count: Integer;
  Log: string;
  Items: array of Integer;
  P: TPoint;

  procedure Add(const S: string);
  begin
    Log := Log + Prefix + S + ';';
    Inc(Count);
  end;

  // The middle level does not use the outer variables itself; it passes the static link on
  procedure Middle;

    procedure Deep(K: Integer);
    begin
      Add('deep');
      SetLength(Items, Length(Items) + 1);
      Items[High(Items)] := N * K;
      P.X := P.X + K;
      Result := Log;
    end;

  begin
    Deep(1);
    Deep(2);
  end;

  function Sum: Integer;
  var
    I: Integer;
  begin
    Result := 0;
    for I := 0 to High(Items) do
      Result := Result + Items[I];
  end;

begin
  Count := 0;
  Log := '';
  Items := nil;
  P := Start;
  Add('a');
  Middle;
  WriteLn('log: ', Result);
  WriteLn('count: ', Count, ' sum: ', Sum, ' p: ', P.X, ',', P.Y);
  Result := Result + '!';
end;

// Recursion of a nested routine, a nested function assigning to the enclosing function's name,
// and a nested routine passing an outer variable to a var parameter
function Factorials(N: Integer): Integer;
var
  Total: Integer;

  procedure Swap(var A, B: Integer);
  var
    T: Integer;
  begin
    T := A;
    A := B;
    B := T;
  end;

  function Fact(K: Integer): Integer;
  begin
    if K <= 1 then
      Fact := 1
    else
      Fact := K * Fact(K - 1);
  end;

  procedure Walk(K: Integer);
  var
    Other: Integer;
  begin
    if K > N then
      Exit;
    Total := Total + Fact(K);
    Other := 0;
    Swap(Total, Other);
    Swap(Total, Other);
    Factorials := Total;
    Walk(K + 1);
  end;

begin
  Total := 0;
  Factorials := 0;
  Walk(1);
end;

// Mutually recursive nested routines through a forward declaration
procedure Parity(N: Integer);
var
  Steps: Integer;

  function IsOdd(K: Integer): Boolean; forward;

  function IsEven(K: Integer): Boolean;
  begin
    Inc(Steps);
    if K = 0 then
      IsEven := True
    else
      IsEven := IsOdd(K - 1);
  end;

  function IsOdd(K: Integer): Boolean;
  begin
    Inc(Steps);
    if K = 0 then
      IsOdd := False
    else
      IsOdd := IsEven(K - 1);
  end;

begin
  Steps := 0;
  WriteLn(N, ' even: ', IsEven(N), ' steps: ', Steps);
end;

// The var and out parameters of an enclosing routine are the caller's variables
procedure Params(var V: string; out O: Integer; W: Integer);

  procedure Fill;
  begin
    V := V + '+';
    O := W * 10;
  end;

begin
  Fill;
  Fill;
end;

// A nested routine that uses nothing of the enclosing routine, and overloads
procedure Plain;

  function Twice(X: Integer): Integer; overload;
  begin
    Twice := X * 2;
  end;

  function Twice(const S: string): string; overload;
  begin
    Twice := S + S;
  end;

begin
  WriteLn(Twice(21), ' ', Twice('ab'));
end;

var
  Text: string;
  Num: Integer;
begin
  WriteLn(Outer(7, '>'));
  Text := 'v';
  Params(Text, Num, 4);
  WriteLn(Text, ' ', Num);
  WriteLn(Factorials(5));
  Parity(4);
  Parity(3);
  Plain;
end.
