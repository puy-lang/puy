program EqualsHashCode;

// `TObject.Equals` and `TObject.GetHashCode` (equality.md 3.1, 8.2; objects.md chapter 5): the
// default `Equals` is identity (the same instance, another one, `nil`), the default
// `GetHashCode` gives one value for one instance (the values themselves differ between
// processes, so they are not written), overrides are called virtually through a `TObject`
// variable, `inherited Equals` / `inherited GetHashCode` reach the built-in bodies, and the
// built-in bodies are reached through a reference counted class, an exception, a method
// reference and an interface whose methods they implement.

uses
  System.SysUtils;

type
  TPlain = class
  end;

  TColor = class(TRefObject)
  public
    R, G: Integer;
    constructor Create(AR, AG: Integer);
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  // Overrides both with the built-in bodies of `TObject` (`inherited`)
  TSame = class(TRefObject)
  public
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  IKey = interface
    function Equals(Obj: TObject): Boolean;
    function GetHashCode: Int64;
  end;

  TKeyed = class(TRefObject, IKey)
  end;

constructor TColor.Create(AR, AG: Integer);
begin
  R := AR;
  G := AG;
end;

function TColor.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TColor) and (TColor(Obj).R = R) and (TColor(Obj).G = G);
end;

function TColor.GetHashCode: Int64;
begin
  Result := HashCombine(R, G);
end;

function TSame.Equals(Obj: TObject): Boolean;
begin
  Result := inherited Equals(Obj);
end;

function TSame.GetHashCode: Int64;
begin
  Result := inherited GetHashCode;
end;

var
  P, Q: TPlain;
  O1, O2, O3: TObject;
  S1, S2: TSame;
  E: Exception;
  F: function(Obj: TObject): Boolean of object;
  H: function: Int64 of object;
  K: TKeyed;
  I: IKey;
begin
  P := TPlain.Create;
  Q := TPlain.Create;
  WriteLn('identity: ', P.Equals(P), ' ', P.Equals(Q), ' ', P.Equals(nil));
  WriteLn('hash: ', P.GetHashCode = P.GetHashCode, ' ', P.GetHashCode = Q.GetHashCode);

  O1 := TColor.Create(1, 2);
  O2 := TColor.Create(1, 2);
  O3 := TColor.Create(2, 1);
  WriteLn('override: ', O1.Equals(O2), ' ', O1.Equals(O3), ' ', O1.Equals(P));
  WriteLn('override hash: ', O1.GetHashCode = O2.GetHashCode, ' ', O1.GetHashCode = HashCombine(1, 2));

  S1 := TSame.Create;
  S2 := TSame.Create;
  WriteLn('inherited: ', S1.Equals(S1), ' ', S1.Equals(S2), ' ', S1.Equals(O1));
  WriteLn('inherited hash: ', S1.GetHashCode = S1.GetHashCode, ' ', S1.GetHashCode = S2.GetHashCode);
  O3 := S1;
  WriteLn('through TObject: ', O3.Equals(S1), ' ', O3.GetHashCode = S1.GetHashCode);

  E := Exception.Create('x');
  WriteLn('exception: ', E.Equals(E), ' ', E.Equals(P), ' ', E.GetHashCode = E.GetHashCode);

  F := P.Equals;
  H := P.GetHashCode;
  WriteLn('method reference: ', F(P), ' ', F(Q), ' ', H() = P.GetHashCode);
  F := O1.Equals;
  WriteLn('method reference override: ', F(O2), ' ', F(P));
  K := TKeyed.Create;
  I := K;
  WriteLn('interface: ', I.Equals(K), ' ', I.Equals(P), ' ', I.GetHashCode = K.GetHashCode);
  P.Free;
  Q.Free;
end.
