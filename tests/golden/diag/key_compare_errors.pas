program KeyCompareErrors;

// Errors of KeyCompare (collections.md 10.2, 12.4): called with a type written out that has no
// default order (a class, an interface, a record without its own LessThan or with one of other
// operands, a set, an array, a procedural type, a class reference) it is E0703 with a note on
// passing a comparer; like KeyEquals it cannot be a procedural value (E0752), its type argument
// is inferred (E0763) and it is not a constant expression.

type
  TPlain = record
    X: Integer;
  end;

  TOther = record
    X: Integer;
    class operator LessThan(const A: TOther; B: Integer): Boolean;
  end;

  TSmall = set of 0..7;
  TCmp = function(const A, B: Integer): Integer;

const
  C = KeyCompare(1, 2);

class operator TOther.LessThan(const A: TOther; B: Integer): Boolean;
begin
  Result := A.X < B;
end;

var
  O: TObject;
  I: IInterface;
  P: TPlain;
  Q: TOther;
  S: TSmall;
  A: array of Integer;
  F: TCmp;
  R: TClass;
  N: Integer;
begin
  N := KeyCompare(O, O);
  N := KeyCompare(I, I);
  N := KeyCompare(P, P);
  N := KeyCompare(Q, Q);
  N := KeyCompare(S, S);
  N := KeyCompare(A, A);
  N := KeyCompare(F, F);
  N := KeyCompare<TClass>(R, R);
  F := KeyCompare;
  N := KeyCompare(1, 2.5);
  N := KeyCompare(1);
end.
