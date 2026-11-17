program GenericSystemQualified;

// The generic types of `System` qualified with the unit's name (generics.md 6.2): `TArray`,
// `TProc` and `TFunc` in a type and in an expression (`Default`, 1.4). `System.TArray<T>` is the
// built-in dynamic array even where the program declares its own `TArray<T>`.

type
  TArray<T> = record
    X: T;
  end;

var
  A: System.TArray<Integer>;
  Own: TArray<Integer>;
  P: System.TProc<Integer>;
  F: System.TFunc<Integer, string>;
  G: System.TFunc<Integer>;

begin
  A := [1, 2, 3];
  Own.X := 7;
  P := procedure(N: Integer)
    begin
      WriteLn('P ', N);
    end;
  F := function(N: Integer): string
    begin
      Result := 'F' + Chr(Ord('0') + N);
    end;
  P(Length(A));
  WriteLn(F(Own.X));
  G := Default(System.TFunc<Integer>);
  WriteLn(Assigned(G));
  A := Default(System.TArray<Integer>);
  WriteLn(Length(A));
end.
