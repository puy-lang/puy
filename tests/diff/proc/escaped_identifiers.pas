program EscapedIdentifiers;

// An identifier written with `&` names the same thing as the identifier without it
// (lexical.md 3.2): variables, constants, types, routines, fields, methods and properties
// declared with `&` are found without it and the other way round. A reserved word needs the
// `&` everywhere; `&Result` declares a member that is called as `X.Result` (semantics.md 4.1).
// Type parameters, nested types in a qualified name and a forward-declared interface are
// matched the same way.

const
  &Limit = 3;

type
  &TPair = record
    &First: Integer;
    &type: Integer;
    function &Sum: Integer;
    function &Result(const Pattern: string): string;
  end;

  TBox = class
  private
    FValue: Integer;
  public
    type
      &TInner = record
        X: Integer;
      end;
    class var &Result: Integer;
    property &Value: Integer read FValue write FValue;
  end;

  THolder<&T> = record
    Item: T;
  end;

  &IShape = interface;

  IShape = interface
    function Area: Integer;
  end;

  TSquare = class(TInterfacedObject, &IShape)
    function Area: Integer;
  end;

function TSquare.Area: Integer;
begin
  Result := 9;
end;

function TPair.Sum: Integer;
begin
  Result := First + &type;
end;

function TPair.&Result(const Pattern: string): string;
begin
  Result := '<' + Pattern + '>';
end;

function &Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

var
  &Count: Integer;
  P: TPair;
  B: TBox;
  Inner: TBox.&TInner;
  H: THolder<Integer>;
  S: IShape;
begin
  Count := Limit;
  WriteLn(&Count, ' ', Count, ' ', &Limit);
  WriteLn(Twice(4), ' ', &twice(5));
  P.First := 1;
  P.&First := P.First + 1;
  P.&type := 10;
  WriteLn(P.First, ' ', P.Sum, ' ', P.&Sum);
  WriteLn(P.Result('a'), ' ', P.&Result('b'));
  B := TBox.Create;
  B.Value := 7;
  TBox.Result := 4;
  WriteLn(B.&Value, ' ', TBox.Result, ' ', TBox.&result);
  B.Free;
  Inner.X := 5;
  H.Item := 6;
  WriteLn(Inner.X, ' ', H.Item);
  S := TSquare.Create;
  WriteLn(S.Area);
end.
