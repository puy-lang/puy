program GenericTypeKinds;

// The generic types other than classes and records (generics.md 2.1, 6.2): a generic interface
// implemented by a class and called through instantiations (with a property, `Supports` and
// `as`), generic interfaces declared forward and inheriting from an instantiation, generic
// procedural types of the three families, the `System` types `TProc<…>`, `TFunc<…>` and
// `TPredicate<T>` holding anonymous methods, generic dynamic and static array types (a dynamic
// one is the same type as any other dynamic array of its element type), an alias of an
// instantiation, `class of` an instantiation, and a generic exception class caught by `on E:`.

uses System.SysUtils;

type
  ISource<T> = interface;

  IGetter<T> = interface
    function Get: T;
    property Value: T read Get;
  end;

  ISource<T> = interface(IGetter<T>)
    function Next: ISource<T>;
  end;

  TConst = class(TInterfacedObject, IGetter<Integer>)
    function Get: Integer;
  end;

  TChain = class(TInterfacedObject, ISource<string>)
    FName: string;
    FNext: TChain;
    constructor Create(const AName: string; ANext: TChain);
    function Get: string;
    function Next: ISource<string>;
  end;

  TMap<T, R> = reference to function(const X: T): R;
  TCmp<T> = function(const A, B: T): Integer;
  TEvent<T> = procedure(const X: T) of object;
  TRow<T> = array of T;
  TFour<T> = array[0..3] of T;
  TBox<T> = record
    Value: T;
  end;
  TIntBox = TBox<Integer>;

  TPrinter = class
    Prefix: string;
    procedure Print(const X: Double);
  end;

  EWrapped<T> = class(Exception)
    Data: T;
  end;
  TWrappedClass = class of EWrapped<Integer>;

function TConst.Get: Integer;
begin
  Result := 42;
end;

constructor TChain.Create(const AName: string; ANext: TChain);
begin
  FName := AName;
  FNext := ANext;
end;

function TChain.Get: string;
begin
  Result := FName;
end;

function TChain.Next: ISource<string>;
begin
  Result := FNext;
end;

procedure TPrinter.Print(const X: Double);
begin
  WriteLn(Prefix, X:0:1);
end;

function CompareInts(const A, B: Integer): Integer;
begin
  Result := A - B;
end;

var
  G: IGetter<Integer>;
  S: ISource<string>;
  GS: IGetter<string>;
  M: TMap<Integer, string>;
  C: TCmp<Integer>;
  E: TEvent<Double>;
  Printer: TPrinter;
  P0: TProc;
  P1: TProc<Integer>;
  P2: TProc<string, Integer>;
  F0: TFunc<Integer>;
  F1: TFunc<Integer, string>;
  F3: TFunc<Integer, Integer, Integer, Integer>;
  Pr: TPredicate<string>;
  Row: TRow<string>;
  Plain: array of string;
  Four: TFour<Double>;
  B: TIntBox;
  WC: TWrappedClass;
  I: Integer;
begin
  G := TConst.Create;
  WriteLn(G.Get, ' ', G.Value);
  S := TChain.Create('a', TChain.Create('b', nil));
  WriteLn(S.Get, S.Next.Get, ' ', S.Next.Value);
  GS := S;
  WriteLn(GS.Get, ' ', Supports(GS, ISource<string>));
  S := GS as ISource<string>;
  WriteLn(S.Next.Get);

  M := function(const X: Integer): string
    begin
      Result := 'n' + IntToStr(X);
    end;
  WriteLn(M(3));
  C := CompareInts;
  WriteLn(C(5, 2));
  Printer := TPrinter.Create;
  Printer.Prefix := 'value ';
  E := Printer.Print;
  E(2.5);
  Printer.Free;

  P0 := procedure
    begin
      WriteLn('TProc');
    end;
  P1 := procedure(Arg1: Integer)
    begin
      WriteLn('TProc<Integer> ', Arg1);
    end;
  P2 := procedure(A: string; N: Integer)
    begin
      WriteLn('TProc<string, Integer> ', A, N);
    end;
  F0 := function: Integer
    begin
      Result := 7;
    end;
  F1 := function(N: Integer): string
    begin
      Result := IntToStr(N * 2);
    end;
  F3 := function(A, B, C: Integer): Integer
    begin
      Result := A + B + C;
    end;
  Pr := function(Arg1: string): Boolean
    begin
      Result := Arg1 <> '';
    end;
  P0();
  P1(3);
  P2('x', 4);
  WriteLn(F0(), ' ', F1(5), ' ', F3(1, 2, 3), ' ', Pr(''), ' ', Pr('a'));

  SetLength(Row, 2);
  Row[0] := 'p';
  Row[1] := 'q';
  Plain := Row;
  WriteLn(Length(Plain), Plain[1]);
  for I := 0 to 3 do
    Four[I] := I / 2;
  WriteLn(Four[3]:0:2);
  B.Value := 9;
  WriteLn(B.Value);

  WC := EWrapped<Integer>;
  WriteLn(WC.ClassName);
  try
    raise EWrapped<Integer>.Create('boom');
  except
    on X: EWrapped<string> do
      WriteLn('string');
    on X: EWrapped<Integer> do
      WriteLn('Integer ', X.Message, ' ', X.Data);
  end;
end.
