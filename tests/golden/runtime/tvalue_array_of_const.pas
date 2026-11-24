program TValueArrayOfConst;

// A user's `array of const` parameter (rtti.md chapter 4): each element a TValue of its own type
// (a one-character literal a string, nil the empty value, a TValue as it is), passed on as an
// array of TValue, and Format of it (4.2)

uses System.SysUtils;

type
  TColor = (Red, Green);
  TPoint = record
    X: Integer;
  end;
  TFoo = class
  end;

procedure Show(const Args: array of const);
var
  I: Integer;
begin
  Write(Length(Args), ':');
  for I := 0 to High(Args) do
    Write(' ', Args[I].ToString, '/', Ord(Args[I].Kind));
  WriteLn;
end;

procedure Pass(const Args: array of const);
begin
  Show(Args);
end;

function Fmt(const S: string; const Args: array of const): string;
begin
  Result := Format(S, Args);
end;

var
  F: TFoo;
  P: TPoint;
  T: TValue;
  Vals: TArray<TValue>;
begin
  F := TFoo.Create;
  P.X := 1;
  T := TValue.From<Integer>(9);
  Show([1, 'a', 'bc', 2.5, True, F, nil, TColor.Green, P, T]);
  Show([]);
  Pass([5000000000, Int64(1)]);
  Vals := [T, 'x'];
  Show(Vals);
  WriteLn(Fmt('%d %s %.2f', [3, 'x', 1.5]));
  try
    WriteLn(Fmt('%d', [P]));
  except
    on E: EConvertError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  F.Free;
end.
