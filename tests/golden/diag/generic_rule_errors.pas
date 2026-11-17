program GenericRuleErrors;

// The rules of generics.md left out by the other cases: type arguments on a helper's parent
// (E0761 for a helper's name, E0753 for a generic type that is no helper; 1.3), an anonymous type
// as a type argument (only E0404; 1.3), `dynamic` / `abstract` / `override` methods with type
// parameters (E0757; 2.1), two generic types with the same number of type parameters (E0701;
// 2.3), the types that do not satisfy `class` / `record` and a descendant interface that
// satisfies an interface constraint (3.1), `record` with a class type (E0758; 3.2), a `case`
// selector, a conversion to `T` and `T.Create` with arguments (3.3), a type parameter passed to a
// generic routine's constraint, written or inferred (E0759; 3.5), the concrete part of a mixed
// type argument (3.5), no covariance (4.1), too few type arguments (E0761; 5.1), `TFunc` with
// six type arguments and `TProc` taking its arguments by value (6.2), and `TypeInfo(T)` (E3200
// planned for M5; chapter 10).

type
  TPlainHelper = class helper for TObject
  end;

  TBox<T> = class
    V: T;
  end;

  TH1 = class helper(TPlainHelper<Integer>) for TObject
  end;

  TH2 = class helper(TBox<Integer>) for TObject
  end;

  TDup<T> = record
  end;
  TDup<U> = record
  end;

  TVirtuals = class
    procedure D<T>; dynamic;
    procedure A<T>; virtual; abstract;
    function ToString<T>: string; override;
  end;

  TBase = class
  end;
  TFoo = class(TBase)
  end;

  TR1<T: record, TBase> = class
  end;
  TR2<T: TBase, record> = class
  end;

  IA = interface
    procedure Run;
  end;
  IB = interface(IA)
  end;

  TP = procedure;

  TRec<T: record> = record
  end;
  TCls<T: class> = record
  end;
  TIntf<T: IA> = record
  end;

  TPair<A, B> = record
    X: A;
    Y: B;
  end;
  TClsOnly<U: class> = class
  end;
  TMixed<T: class> = class
    Q: TPair<TClsOnly<Integer>, T>;
  end;

  TBag<T> = class
    FItem: T;
    procedure Run;
  end;
  TMk<T: class, constructor> = class
    procedure Run;
  end;

function Pick<U: class>(const X: U): U;
begin
  Result := X;
end;

procedure PassOn<T>(const X: T);
begin
  Pick<T>(X);
  Pick(X);
end;

function Pair<K, V>(const A: K; const B: V): Integer;
begin
  Result := 0;
end;

procedure TBag<T>.Run;
begin
  case FItem of
    1: ;
  end;
  FItem := 5;
  WriteLn(TypeInfo(T) = nil);
end;

procedure TMk<T>.Run;
begin
  T.Create(1);
end;

var
  B: TBox<array of Integer>;
  R1: TRec<TArray<Integer>>;
  R2: TRec<TP>;
  R3: TRec<IA>;
  C1: TCls<IA>;
  C2: TCls<TClass>;
  I1: TIntf<IB>;
  LF: TBox<TFoo>;
  LB: TBox<TBase>;
  F6: TFunc<Integer, Integer, Integer, Integer, Integer, string>;
  P2: TProc<Integer, string>;
  M: TPair<Integer>;
begin
  LB := LF;
  WriteLn(Pair<Integer>(1, 2));
  P2 := procedure(const A: Integer; B: string)
    begin
    end;
end.
