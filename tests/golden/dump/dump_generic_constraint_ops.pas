program DumpGenericConstraintOps;

// `=` / `<>` of two values of a type parameter with the `class` constraint, and with `nil`, are
// the identity comparison (generics.md 3.3, 4.3): the instantiation for `TNode`, whose
// `class operator Equal` says every two nodes are equal, compares the references with the
// instruction of its static type and does not call the operator. `T.Create` of the generic body
// has no constructor yet (`create T -` in the `hir`); the instantiation calls the one the
// concrete class has (here the built-in `TObject.Create`).

type
  TNode = class(TRefObject)
    class operator Equal(const A, B: TNode): Boolean;
    function GetHashCode: Int64; override;
  end;

  TPair<T: class, constructor> = class
    FA, FB: T;
    function Same: Boolean;
    function HasA: Boolean;
    function Make: T;
  end;

class operator TNode.Equal(const A, B: TNode): Boolean;
begin
  Result := True;
end;

function TNode.GetHashCode: Int64;
begin
  Result := 0;
end;

function TPair<T>.Same: Boolean;
begin
  Result := FA = FB;
end;

function TPair<T>.HasA: Boolean;
begin
  Result := FA <> nil;
end;

function TPair<T>.Make: T;
begin
  Result := T.Create;
end;

var
  P: TPair<TNode>;
begin
  P := TPair<TNode>.Create;
  P.FA := P.Make;
  P.FB := TNode.Create;
  WriteLn(P.FA = P.FB, ' ', P.Same, ' ', P.HasA);
  P.Free;
end.
