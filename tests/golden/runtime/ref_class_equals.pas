program RefClassEquals;

// The `Equals` generated for a reference counted class declaring its own `Equal`
// (equality.md 3.3): called through the class's type, `TObject` and an interface value (as
// `TObject`), it gives the operator's result, and `False` for `nil` and for other classes. A
// descendant inherits it; one that declares `Equal` again gets its own, and the two are not
// equal from either side. `inherited Equals` reaches the generated one, and each instantiation of
// a generic class gets its own; a class inheriting from an instantiation and declaring `Equal`
// again is not equal to the instantiation from either side (generics.md 2.4).

type
  IValue = interface
    function GetV: Integer;
  end;

  TNum = class(TRefObject, IValue)
    V: Integer;
    constructor Create(AV: Integer);
    function GetV: Integer;
    class operator Equal(const A, B: TNum): Boolean;
    function GetHashCode: Int64; override;
  end;

  // Inherits the generated `Equals` of `TNum`.
  TNamedNum = class(TNum)
    Name: string;
  end;

  // Declares `Equal` again: its own generated `Equals`.
  TExactNum = class(TNum)
    class operator Equal(const A, B: TExactNum): Boolean;
    function GetHashCode: Int64; override;
  end;

  // Overrides both without `Equal`: `inherited Equals` is the generated one of `TNum`.
  TLogged = class(TNum)
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  TOther = class(TRefObject)
  end;

  TBox<T> = class(TRefObject)
    V: T;
    constructor Create(AV: T);
    class operator Equal(const A, B: TBox<T>): Boolean;
    function GetHashCode: Int64; override;
  end;

  TIntBox = class(TBox<Integer>)
    class operator Equal(const A, B: TIntBox): Boolean;
    function GetHashCode: Int64; override;
  end;

constructor TNum.Create(AV: Integer);
begin
  V := AV;
end;

function TNum.GetV: Integer;
begin
  Result := V;
end;

class operator TNum.Equal(const A, B: TNum): Boolean;
begin
  WriteLn('  TNum.Equal(', A.V, ', ', B.V, ')');
  Result := A.V = B.V;
end;

function TNum.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TExactNum.Equal(const A, B: TExactNum): Boolean;
begin
  WriteLn('  TExactNum.Equal(', A.V, ', ', B.V, ')');
  Result := A.V = B.V;
end;

function TExactNum.GetHashCode: Int64;
begin
  Result := V;
end;

function TLogged.Equals(Obj: TObject): Boolean;
begin
  WriteLn('  TLogged.Equals');
  Result := inherited Equals(Obj);
end;

function TLogged.GetHashCode: Int64;
begin
  Result := inherited GetHashCode;
end;

constructor TBox<T>.Create(AV: T);
begin
  V := AV;
end;

class operator TBox<T>.Equal(const A, B: TBox<T>): Boolean;
begin
  WriteLn('  TBox.Equal');
  Result := True;
end;

function TBox<T>.GetHashCode: Int64;
begin
  Result := 0;
end;

class operator TIntBox.Equal(const A, B: TIntBox): Boolean;
begin
  WriteLn('  TIntBox.Equal');
  Result := True;
end;

function TIntBox.GetHashCode: Int64;
begin
  Result := 1;
end;

var
  A, B, C: TNum;
  N: TNamedNum;
  E: TExactNum;
  L: TLogged;
  O1, O2: TObject;
  I1, I2: IValue;
  BI1, BI2: TBox<Integer>;
  BS: TBox<string>;
  IB: TIntBox;
begin
  A := TNum.Create(1);
  B := TNum.Create(1);
  C := TNum.Create(2);
  WriteLn('TNum:');
  WriteLn(A.Equals(B), ' ', A.Equals(C), ' ', A = B);
  WriteLn('TObject:');
  O1 := A;
  O2 := B;
  WriteLn(O1.Equals(O2));
  WriteLn(O1.Equals(nil), ' ', O1.Equals(TOther.Create), ' ', TOther.Create.Equals(O1));
  WriteLn('interface:');
  I1 := A;
  I2 := B;
  WriteLn((I1 as TObject).Equals(I2 as TObject));
  WriteLn('inherited by a descendant:');
  N := TNamedNum.Create(1);
  WriteLn(A.Equals(N), ' ', N.Equals(A));
  WriteLn('declared again by a descendant:');
  E := TExactNum.Create(1);
  WriteLn(A.Equals(E), ' ', E.Equals(A), ' ', N.Equals(E), ' ', E.Equals(N));
  WriteLn(E.Equals(TExactNum.Create(1)));
  O1 := E;
  WriteLn(O1.Equals(TExactNum.Create(2)));
  WriteLn('inherited Equals:');
  L := TLogged.Create(1);
  WriteLn(L.Equals(A));
  WriteLn('instantiations:');
  BI1 := TBox<Integer>.Create(1);
  BI2 := TBox<Integer>.Create(2);
  BS := TBox<string>.Create('x');
  WriteLn(BI1.Equals(BI2), ' ', BI1.Equals(BS), ' ', BS.Equals(BI1));
  WriteLn('a class inheriting from an instantiation:');
  IB := TIntBox.Create(1);
  WriteLn(BI1.Equals(IB), ' ', IB.Equals(BI1), ' ', BS.Equals(IB), ' ', IB.Equals(TIntBox.Create(2)));
end.
