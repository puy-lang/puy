program GenericNestedTypeExpr;

// A type nested in an instantiation can be named in an expression right after the type
// arguments (`TOuter<Integer>.TInner<string>.Create`; generics.md 1.4, 2.5): a nested generic
// type with its own type arguments, a nested type that is not generic, and one nested in a
// generic record. It is the same type as the one named through an alias of the instantiation,
// so the results are assignable to each other and share the class variables.

type
  TOuter<T> = class
  public
    type
      TInner<U> = class
      public
        type
          TDeep = class
          public
            class function Describe: string;
          end;
      public
        class var Made: Integer;
        Left: T;
        Right: U;
        constructor Create;
        class function Make(const L: T; const R: U): TInner<U>;
        class function Pick<V>(const A, B: V; First: Boolean): V;
      end;
      TCell = class
      public
        Value: T;
        class function Kind: string;
      end;
  end;

  TBox<T> = record
  public
    type
      TTag<U> = class
      public
        class function Show(const X: T; const Y: U): string;
      end;
  end;

  TIntOuter = TOuter<Integer>;

constructor TOuter<T>.TInner<U>.Create;
begin
  Inc(Made);
end;

class function TOuter<T>.TInner<U>.Make(const L: T; const R: U): TInner<U>;
begin
  Result := TInner<U>.Create;
  Result.Left := L;
  Result.Right := R;
end;

class function TOuter<T>.TInner<U>.Pick<V>(const A, B: V; First: Boolean): V;
begin
  if First then
    Result := A
  else
    Result := B;
end;

class function TOuter<T>.TInner<U>.TDeep.Describe: string;
begin
  Result := 'deep';
end;

class function TOuter<T>.TCell.Kind: string;
begin
  Result := 'cell';
end;

class function TBox<T>.TTag<U>.Show(const X: T; const Y: U): string;
begin
  Result := 'tag';
end;

var
  A: TOuter<Integer>.TInner<string>;
  B: TIntOuter.TInner<string>;
  C: TOuter<Integer>.TCell;
  D: TOuter<Double>.TInner<string>;
begin
  // A constructor and a class function, through the instantiation and through the alias
  A := TOuter<Integer>.TInner<string>.Create;
  A.Left := 1;
  A.Right := 'one';
  B := A;
  WriteLn(B.Left, ' ', B.Right);
  B.Free;
  B := TOuter<Integer>.TInner<string>.Make(2, 'two');
  A := B;
  WriteLn(A.Left, ' ', A.Right);
  A.Free;
  A := TIntOuter.TInner<string>.Make(3, 'three');
  WriteLn(A.Left, ' ', A.Right);
  A.Free;

  // The class variable is shared by both forms and is another one for `TOuter<Double>`
  D := TOuter<Double>.TInner<string>.Create;
  WriteLn(TOuter<Integer>.TInner<string>.Made, ' ', TIntOuter.TInner<string>.Made, ' ',
    TOuter<Double>.TInner<string>.Made);
  D.Free;

  // A generic method of the nested generic type, and a type nested in it
  WriteLn(TOuter<Integer>.TInner<string>.Pick<Char>('x', 'y', False));
  WriteLn(TOuter<Integer>.TInner<string>.TDeep.Describe);

  // A nested type that is not generic
  C := TOuter<Integer>.TCell.Create;
  C.Value := 5;
  WriteLn(C.Value, ' ', TOuter<Integer>.TCell.Kind);
  C.Free;

  // A generic class nested in a generic record
  WriteLn(TBox<Integer>.TTag<string>.Show(1, 'a'));
end.
