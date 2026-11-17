program ValueEqualityEdges;

// The edges of `[ValueEquality]` and of `Equals` / `GetHashCode` (equality.md 1.3, 2.6, 3.3,
// 3.4): a class with no fields to compare equals any instance of its class; `=` stays identity;
// the class is not hashed, so a child with the same field values hashes alike but is not equal; a
// descendant declaring its own `Equal` moves to the generated `Equals` of 3.3; `[weak]` fields
// are left out when all fields are compared; the attribute names ignore case; a cycle of fields
// raises EStackOverflow; an instance being destroyed is not treated as destroyed, so its
// `Equals` / `GetHashCode` are called; and a generated `Equals` given a freed instance raises
// EAccessViolation.

uses
  System.SysUtils;

type
  TKey = class
    Id: Integer;
    Other: TKey;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
    destructor Destroy; override;
  end;

  [ValueEquality]
  TNone = class(TRefObject)
  end;

  [ValueEquality]
  TVal = class(TRefObject)
    V: Integer;
  end;

  TValChild = class(TVal)
  end;

  TValEq = class(TVal)
    class operator Equal(const A, B: TValEq): Boolean;
    function GetHashCode: Int64; override;
  end;

  [ValueEquality]
  TWeakHolder = class
    V: Integer;
    [weak] W: TVal;
  end;

  [valueequality]
  TLower = class
    [equalitymember] V: Integer;
    W: Integer;
  end;

  [ValueEquality]
  TNode = class
    V: Integer;
    Next: TNode;
  end;

  TNum = class(TRefObject)
    V: Integer;
    class operator Equal(const A, B: TNum): Boolean;
    function GetHashCode: Int64; override;
  end;

  TPlain = class
  end;

function TKey.Equals(Obj: TObject): Boolean;
begin
  WriteLn('  Equals called');
  Result := (Obj is TKey) and (TKey(Obj).Id = Id);
end;

function TKey.GetHashCode: Int64;
begin
  WriteLn('  GetHashCode called');
  Result := Id;
end;

destructor TKey.Destroy;
begin
  if Other <> nil then
  begin
    WriteLn('in destructor: ', KeyEquals<TKey>(Self, Other));
    WriteLn('hash in destructor: ', KeyHash<TKey>(Self) = Id);
  end;
  inherited;
end;

class operator TValEq.Equal(const A, B: TValEq): Boolean;
begin
  WriteLn('  TValEq.Equal');
  Result := True;
end;

function TValEq.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TNum.Equal(const A, B: TNum): Boolean;
begin
  Result := A.V = B.V;
end;

function TNum.GetHashCode: Int64;
begin
  Result := V;
end;

var
  A, B: TKey;
  N1, N2: TNone;
  V1, V2: TVal;
  C1: TValChild;
  Q1, Q2: TValEq;
  W1, W2: TWeakHolder;
  Lo1, Lo2: TLower;
  L1, L2: TNode;
  Num: TNum;
  P: TPlain;
begin
  A := TKey.Create;
  B := TKey.Create;
  A.Id := 1;
  B.Id := 1;
  A.Other := B;
  A.Free;
  B.Free;

  N1 := TNone.Create;
  N2 := TNone.Create;
  WriteLn('no fields: ', N1.Equals(N2), ' ', N1.GetHashCode = N2.GetHashCode);
  V1 := TVal.Create;
  V2 := TVal.Create;
  V1.V := 1;
  V2.V := 1;
  WriteLn('= stays identity: ', V1 = V2, ' ', KeyEquals(V1, V2));
  C1 := TValChild.Create;
  C1.V := 1;
  WriteLn('child, same fields: ', V1.Equals(C1), ' ', V1.GetHashCode = C1.GetHashCode);
  Q1 := TValEq.Create;
  Q2 := TValEq.Create;
  Q1.V := 1;
  Q2.V := 2;
  WriteLn('descendant with Equal: ', Q1.Equals(Q2));

  W1 := TWeakHolder.Create;
  W2 := TWeakHolder.Create;
  W1.W := V1;
  W2.W := V2;
  WriteLn('weak left out: ', W1.Equals(W2), ' ', W1.GetHashCode = W2.GetHashCode);
  W1.Free;
  W2.Free;

  Lo1 := TLower.Create;
  Lo2 := TLower.Create;
  Lo1.V := 1;
  Lo2.V := 1;
  Lo1.W := 1;
  Lo2.W := 2;
  WriteLn('lower-case attributes: ', Lo1.Equals(Lo2));
  Lo1.Free;
  Lo2.Free;

  L1 := TNode.Create;
  L2 := TNode.Create;
  L1.Next := L1;
  L2.Next := L2;
  try
    WriteLn(L1.Equals(L2));
  except
    on E: Exception do
      WriteLn('cycle: ', E.ClassName);
  end;
  L1.Free;
  L2.Free;

  Num := TNum.Create;
  P := TPlain.Create;
  P.Free;
  try
    WriteLn(Num.Equals(P));
  except
    on E: Exception do
      WriteLn('generated Equals, freed: ', E.ClassName);
  end;
  try
    WriteLn(V1.Equals(P));
  except
    on E: Exception do
      WriteLn('value Equals, freed: ', E.ClassName);
  end;
end.
