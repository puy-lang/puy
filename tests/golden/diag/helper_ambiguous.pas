program HelperAmbiguous;

// E0754 (objects.md 10.4): two visible helpers for the same type both have the name, in the
// same step of the search. A method gets a fix per helper that names it (`H.M(X, …)`, 10.5); a
// property cannot be named so, and gets a note instead. A user's helper for `string` declaring
// `Length` is ambiguous with the built-in `TStringHelper` (10.7). A parent and its child helper
// declare the same member, which is not ambiguous; an overload in another helper is (10.4).
// The nearest ancestor's helper wins without E0754. `H.M(…)` with an instance method and a class
// method of the name is E0734 when both forms fit the arguments and E0735 when neither does
// (10.10).

type
  TFoo = class
  end;

  TBar = class(TFoo)
  end;

  TFooA = class helper for TFoo
    function Name: string;
    property Size: Integer read GetSize;
    function GetSize: Integer;
  end;

  TFooB = class helper for TFoo
    function Name: string;
    property Size: Integer read GetSize;
    function GetSize: Integer;
  end;

  TBarHelper = class helper for TBar
    function Name: string;
  end;

  TTextA = record helper for string
    function Pad(N: Integer): string;
    property Length: Integer read Len;
    function Len: Integer;
  end;

  TTextB = record helper for string
    function Pad(const S: string): string;
  end;

  TTextC = record helper(TTextB) for string
  end;

  TNumHelper = record helper for Integer
    function Add(B: Integer): Integer; overload;
    class function Add(A, B: Integer): Integer; static; overload;
  end;

function TFooA.Name: string; begin Result := 'A'; end;
function TFooA.GetSize: Integer; begin Result := 1; end;
function TFooB.Name: string; begin Result := 'B'; end;
function TFooB.GetSize: Integer; begin Result := 2; end;
function TBarHelper.Name: string; begin Result := 'bar'; end;
function TTextA.Pad(N: Integer): string; begin Result := Self; end;
function TTextA.Len: Integer; begin Result := 0; end;
function TTextB.Pad(const S: string): string; begin Result := Self + S; end;
function TNumHelper.Add(B: Integer): Integer; begin Result := Self + B; end;
class function TNumHelper.Add(A, B: Integer): Integer; begin Result := A + B; end;

var
  F: TFoo;
  B: TBar;
  S: string;
begin
  F := TFoo.Create;
  B := TBar.Create;
  WriteLn(F.Name);                                 // E0754 with two fixes
  WriteLn(F.Name());                               // E0754 with two fixes
  WriteLn(F.Size);                                 // E0754: a property, no fixes
  WriteLn(B.Name);                                 // `TBarHelper`, the nearest step
  S := 'abc';
  WriteLn(S.Pad(3));                               // E0754: overloads in two helpers
  WriteLn(S.Length);                               // E0754: also `TStringHelper`
  WriteLn(TTextB.Pad(S, 'x'), TTextC.Pad(S, 'y'));
  WriteLn(TNumHelper.Add(1, 2));                   // E0734: `1.Add(2)` or `Add(1, 2)`
  WriteLn(TNumHelper.Add(1, 2, 3));                // E0735
  F.Free;
  B.Free;
end.
