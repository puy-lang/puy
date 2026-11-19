program GenericNestedInherited;

// A generic type nested in a class is seen from its descendants as a nested type that is not
// generic is (generics.md 2.5): through the descendant's name (`TD.TG<string>`) and by its own
// name in a method of the descendant. Nested in a generic class, it takes the type arguments of
// the instantiation the descendant inherits from (`TE = class(TOuter<Integer>)`), or the open
// ones of a generic descendant (`TF<T> = class(TOuter<T>)`). A descendant that declares a nested
// generic type with the same name and number of type parameters hides the ancestor's
// (`TE2.TInner<Char>` is `TE2`'s own).

type
  TC = class
  type
    TG<T> = class
      V: T;
    end;
  end;

  TD = class(TC)
    procedure Run;
  end;

  TOuter<T> = class
  type
    TInner<U> = class
      A: T;
      B: U;
    end;
  end;

  TE = class(TOuter<Integer>)
    procedure Run;
  end;

  TF<T> = class(TOuter<T>)
    procedure Run;
  end;

  TE2 = class(TE)
  type
    TInner<U> = class
      Z: U;
    end;
    procedure Run;
  end;

procedure TD.Run;
var
  G: TG<Integer>;
begin
  G := TG<Integer>.Create;
  G.V := 7;
  WriteLn(G.V, ' ', G is TC.TG<Integer>);
  G.Free;
end;

procedure TE.Run;
var
  I: TInner<string>;
begin
  I := TInner<string>.Create;
  I.A := 5;
  I.B := 'x';
  WriteLn(I.A, I.B, ' ', I is TOuter<Integer>.TInner<string>);
  I.Free;
end;

procedure TF<T>.Run;
var
  I: TInner<string>;
  O: TObject;
begin
  I := TInner<string>.Create;
  I.A := Default(T);
  I.B := 'y';
  O := I;
  WriteLn(I.B, ' ', O is TOuter<T>.TInner<string>);
  I.Free;
end;

procedure TE2.Run;
var
  I: TInner<string>;
  O: TObject;
begin
  I := TInner<string>.Create;
  I.Z := 'z';
  O := I;
  WriteLn(I.Z, ' ', O is TE2.TInner<string>, ' ', O is TOuter<Integer>.TInner<string>);
  I.Free;
end;

var
  S: TD.TG<string>;
  O: TObject;
  E: TE.TInner<Char>;
  D: TD;
  EE: TE;
  F: TF<Double>;
  X: TE2.TInner<Char>;
  E2: TE2;
begin
  S := TD.TG<string>.Create;
  S.V := 'hi';
  O := S;
  WriteLn(S.V, ' ', O is TC.TG<string>, ' ', O is TD.TG<string>);
  E := TE.TInner<Char>.Create;
  E.A := 3;
  E.B := 'c';
  O := E;
  WriteLn(E.A, E.B, ' ', O is TOuter<Integer>.TInner<Char>, ' ', O is TOuter<Double>.TInner<Char>);
  D := TD.Create;
  D.Run;
  D.Free;
  EE := TE.Create;
  EE.Run;
  EE.Free;
  F := TF<Double>.Create;
  F.Run;
  F.Free;
  X := TE2.TInner<Char>.Create;
  X.Z := 'q';
  O := X;
  WriteLn(X.Z, ' ', O is TE2.TInner<Char>, ' ', O is TOuter<Integer>.TInner<Char>);
  X.Free;
  E2 := TE2.Create;
  E2.Run;
  E2.Free;
  S.Free;
  E.Free;
end.
