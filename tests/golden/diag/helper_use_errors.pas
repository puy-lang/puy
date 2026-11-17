program HelperUseErrors;

// Errors of using helper members (objects.md 10.4, 10.5, 10.8, 13.4): `H.M(X)` with a receiver
// of another type (E0702) or without one (E0705), an instance property through the helper's
// name (E0700), a record helper's method as a value (E0752), an unqualified name in a helper's
// body that only another helper has (E0700, also the built-in `ToString`), and `GetEnumerator`
// in two helpers of the collection's type (E0754). `.` on a built-in function that needs
// arguments calls it first (E0705); a built-in procedure is not a value (E3200).

type
  TFoo = class
  end;

  TBar = class
  end;

  TFooHelper = class helper for TFoo
    procedure Hi;
    function GetP: Integer;
    property P: Integer read GetP;
  end;

  TIntA = record helper for Integer
    function Bump: Integer;
    function Thrice: Integer;
  end;

  TIntB = record helper for Integer
    function Twice: Integer;
    function Six: Integer;
  end;

  TEnum = class
    function MoveNext: Boolean;
    function Current: Integer;
  end;

  TBag = class
  end;

  TBagA = class helper for TBag
    function GetEnumerator: TEnum;
  end;

  TBagB = class helper for TBag
    function GetEnumerator: TEnum;
  end;

procedure TFooHelper.Hi;
begin
end;

function TFooHelper.GetP: Integer;
begin
  Result := 1;
end;

function TIntA.Bump: Integer;
begin
  Result := Self + 1;
end;

function TIntA.Thrice: Integer;
begin
  Result := Self * 3;
end;

function TIntB.Twice: Integer;
begin
  Result := Thrice;                                // E0700: other helpers are not searched
end;

function TIntB.Six: Integer;
var
  S: string;
begin
  S := ToString;                                   // E0700: nor the built-in helper
  Result := Self.Thrice * 2 + Length(S);
end;

function TEnum.MoveNext: Boolean;
begin
  Result := False;
end;

function TEnum.Current: Integer;
begin
  Result := 0;
end;

function TBagA.GetEnumerator: TEnum;
begin
  Result := nil;
end;

function TBagB.GetEnumerator: TEnum;
begin
  Result := nil;
end;

var
  F: TFoo;
  B: TBar;
  Bag: TBag;
  M: function: Integer of object;
  N: Integer;
begin
  F := TFoo.Create;
  B := TBar.Create;
  Bag := TBag.Create;
  N := 1;
  TFooHelper.Hi(B);                                // E0702
  TFooHelper.Hi;                                   // E0705
  TFooHelper.Hi();                                 // E0705
  WriteLn(TFooHelper.P);                           // E0700: an instance member
  M := N.Bump;                                     // E0752
  for var Z in Bag do                              // E0754
    WriteLn(Z);
  WriteLn(Length.ToString);                        // E0705: `Length` is called first
  WriteLn(WriteLn.ToString);                       // E3200: a procedure is not a value
end.
