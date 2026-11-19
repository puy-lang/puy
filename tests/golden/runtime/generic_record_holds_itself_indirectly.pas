program GenericRecordHoldsItselfIndirectly;

// A record may hold itself through an instantiation that does not contain it by value
// (types.md 1.1, generics.md 4.2): a generic class (`TList<TBar>`), a dynamic array
// (`TArray<TBar>`, also as a type argument of a generic record), and a class that holds an
// instantiation of a generic record with the class itself. None of them is E0727.

uses System.Generics.Collections;

type
  TFoo<T> = record
    V: T;
  end;
  TBar = record
    L: TList<TBar>;
    A: TArray<TBar>;
    F: TFoo<TArray<TBar>>;
    N: Integer;
  end;
  TNode = class
    Next: TFoo<TNode>;
    Value: Integer;
  end;
  TTree<T> = record
    Item: T;
    Kids: TArray<TTree<T>>;
  end;

var
  b, c: TBar;
  n, m: TNode;
  t: TTree<Integer>;
begin
  b.N := 3;
  b.L := TList<TBar>.Create;
  c.N := 4;
  b.L.Add(c);
  SetLength(b.A, 1);
  b.A[0].N := 5;
  b.F.V := b.A;
  WriteLn(b.L[0].N, ' ', b.A[0].N, ' ', b.F.V[0].N);
  b.L.Free;

  n := TNode.Create;
  m := TNode.Create;
  n.Value := 1;
  m.Value := 2;
  n.Next.V := m;
  WriteLn(n.Next.V.Value, ' ', m.Next.V = nil);
  m.Free;
  n.Free;

  t.Item := 1;
  SetLength(t.Kids, 2);
  t.Kids[1].Item := 7;
  WriteLn(t.Item, ' ', Length(t.Kids), ' ', t.Kids[1].Item);
end.
