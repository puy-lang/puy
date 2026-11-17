program DefaultParams;

// Parameters with default values (semantics.md 4.8): omitted trailing arguments take the
// default, `const` parameters and every simple type (integer, real, string, Char, Boolean,
// enumeration, set) can have one, a constant expression can be the default, a function whose
// parameters all have defaults is called without parentheses, a `forward` declaration carries
// the defaults, and defaults combine with `overload`.

type
  TAlign = (Left, Right);
  TFlags = set of TAlign;

const
  Width = 4;

function Pad(const S: string; N: Integer = Width * 2; Fill: Char = '.'; Align: TAlign = TAlign.Left): string;
begin
  Result := S;
  while Length(Result) < N do
    if Align = TAlign.Left then
      Result := Result + Fill
    else
      Result := Fill + Result;
end;

function Scale(X: Double = 1.5; Times: Integer = 2): Double;
begin
  Result := X * Times;
end;

function Greeting(const Name: string = 'world'; Loud: Boolean = False): string;
begin
  Result := 'hello, ' + Name;
  if Loud then
    Result := Result + '!';
end;

function CountFlags(F: TFlags = [TAlign.Left]): Integer;
var
  A: TAlign;
begin
  Result := 0;
  for A in F do
    Inc(Result);
end;

procedure Later(N: Integer; const Tag: string = 'tag'); forward;

procedure Show(N: Integer); overload;
begin
  WriteLn('int ', N);
end;

procedure Show(const S: string; Times: Integer = 1); overload;
var
  I: Integer;
begin
  for I := 1 to Times do
    Write(S);
  WriteLn;
end;

procedure Later(N: Integer; const Tag: string);
begin
  WriteLn(Tag, ' ', N);
end;

begin
  WriteLn('[', Pad('ab'), '] [', Pad('ab', 4), '] [', Pad('ab', 5, '*'), '] [', Pad('ab', 5, '-', TAlign.Right), ']');
  WriteLn(Scale:0:2, ' ', Scale():0:2, ' ', Scale(2):0:2, ' ', Scale(2, 5):0:2);
  WriteLn(Greeting, ' / ', Greeting('Puy'), ' / ', Greeting('Puy', True));
  WriteLn(CountFlags, ' ', CountFlags([]), ' ', CountFlags([TAlign.Left, TAlign.Right]));
  Later(1);
  Later(2, 'two');
  Show(5);
  Show('ab');
  Show('cd', 3);
end.
