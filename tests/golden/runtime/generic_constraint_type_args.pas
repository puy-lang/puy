program GenericConstraintTypeArgs;

// Constraints written with type arguments (generics.md 1.1, 3.1, 3.3, 3.5): a constraint can
// have the type parameters of its list as its type arguments, itself included (`T: IOrd<T>`,
// `T: TNode<T>`), and a method's constraint those of its class (`C: IComparer<T>`). An
// instantiation is checked with the constraint's type parameters replaced, and the body finds
// the members through the constraint as written. A type parameter passed on satisfies a
// constraint with type arguments when its own constraints have it. A class can be passed to a
// constraint naming itself, in its parents too (`TCell = class(TNode<TCell>)`; the checks of
// a `type` section wait for its classes' parents and interfaces, 4.2).

type
  IOrd<T: IOrd<T>> = interface
    function Less(const Other: T): Boolean;
  end;

  IComparer<T> = interface
    function Compare(const A, B: T): Integer;
  end;

  TMinOf<T: IOrd<T>> = class
    function Min(const A, B: T): T;
  end;

  TWrap<U: IOrd<U>> = class
    FInner: TMinOf<U>;
    function Min(const A, B: U): U;
  end;

  TNum = class(TRefObject, IOrd<TNum>)
    FValue: Integer;
    constructor Create(V: Integer);
    function Less(const Other: TNum): Boolean;
  end;

  TNode<T: TNode<T>> = class
    FNext: T;
    function Count: Integer;
  end;

  TCell = class(TNode<TCell>)
  end;

  TByValue = class(TRefObject, IComparer<Integer>)
    function Compare(const A, B: Integer): Integer;
  end;

  TPair<T> = class
    FA, FB: T;
    function Pick<C: IComparer<T>>(const Cmp: C): T;
  end;

constructor TNum.Create(V: Integer);
begin
  FValue := V;
end;

function TNum.Less(const Other: TNum): Boolean;
begin
  Result := FValue < Other.FValue;
end;

function TMinOf<T>.Min(const A, B: T): T;
begin
  if A.Less(B) then
    Result := A
  else
    Result := B;
end;

function TWrap<U>.Min(const A, B: U): U;
begin
  Result := FInner.Min(A, B);
end;

function TNode<T>.Count: Integer;
var
  N: T;
begin
  Result := 1;
  N := FNext;
  while N <> nil do
  begin
    Inc(Result);
    N := N.FNext;
  end;
end;

function TByValue.Compare(const A, B: Integer): Integer;
begin
  Result := A - B;
end;

function TPair<T>.Pick<C>(const Cmp: C): T;
begin
  if Cmp.Compare(FA, FB) >= 0 then
    Result := FA
  else
    Result := FB;
end;

function Smallest<T: IOrd<T>>(const Items: array of T): T;
begin
  Result := Items[0];
  for var X in Items do
    if X.Less(Result) then
      Result := X;
end;

var
  M: TMinOf<TNum>;
  W: TWrap<TNum>;
  A, B, C: TCell;
  P: TPair<Integer>;
begin
  M := TMinOf<TNum>.Create;
  WriteLn(M.Min(TNum.Create(5), TNum.Create(2)).FValue);
  W := TWrap<TNum>.Create;
  W.FInner := M;
  WriteLn(W.Min(TNum.Create(3), TNum.Create(8)).FValue);
  WriteLn(Smallest<TNum>([TNum.Create(9), TNum.Create(4), TNum.Create(6)]).FValue);
  WriteLn(Smallest([TNum.Create(9), TNum.Create(1)]).FValue);
  A := TCell.Create;
  B := TCell.Create;
  C := TCell.Create;
  A.FNext := B;
  B.FNext := C;
  WriteLn(A.Count, ' ', C.Count);
  P := TPair<Integer>.Create;
  P.FA := 3;
  P.FB := 8;
  WriteLn(P.Pick<TByValue>(TByValue.Create));
  WriteLn(P.Pick(TByValue.Create));
  P.Free;
  C.Free;
  B.Free;
  A.Free;
  W.Free;
  M.Free;
end.
