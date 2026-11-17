program GenericKinds;

// Generic types beyond classes and records that run the same under FPC and Delphi
// (generics.md 2.1, 2.4, 2.5): a generic interface implemented by a generic class and called
// through it, a generic class inheriting from an instantiation with its type parameter and
// overriding a virtual method, a class that is not generic inheriting from an instantiation,
// a type nested in a generic class using the outer type parameter, `class var`s that each
// instantiation has on its own, and generic dynamic and static array types.

uses System.SysUtils;

type
  IShow<T> = interface
    function Show(const X: T): string;
  end;

  TShower<T> = class(TInterfacedObject, IShow<T>)
  public
    function Show(const X: T): string; virtual;
  end;

  TLoudShower<T> = class(TShower<T>)
  public
    function Show(const X: T): string; override;
  end;

  TIntShower = class(TShower<Integer>)
  public
    function Show(const X: Integer): string; override;
  end;

  TStore<T> = class
  public
    type
      TCell = class
      public
        Value: T;
      end;
  public
    Cell: TCell;
    class var Created: Integer;
  public
    constructor Create(const X: T);
    destructor Destroy; override;
  end;

  TRow<T> = array of T;
  TFour<T> = array[0..3] of T;

function TShower<T>.Show(const X: T): string;
begin
  Result := 'shown';
end;

function TLoudShower<T>.Show(const X: T): string;
begin
  Result := UpperCase(inherited Show(X));
end;

function TIntShower.Show(const X: Integer): string;
begin
  Result := 'int ' + IntToStr(X);
end;

constructor TStore<T>.Create(const X: T);
begin
  Cell := TCell.Create;
  Cell.Value := X;
  Created := Created + 1;
end;

destructor TStore<T>.Destroy;
begin
  Cell.Free;
  inherited;
end;

var
  SI: IShow<Integer>;
  SS: IShow<string>;
  A, B: TStore<Integer>;
  C: TStore<string>;
  Row: TRow<string>;
  Four: TFour<Integer>;
  I: Integer;
begin
  SS := TShower<string>.Create;
  WriteLn(SS.Show('a'));
  SS := TLoudShower<string>.Create;
  WriteLn(SS.Show('b'));
  SI := TIntShower.Create;
  WriteLn(SI.Show(3));

  A := TStore<Integer>.Create(1);
  B := TStore<Integer>.Create(2);
  C := TStore<string>.Create('c');
  WriteLn(A.Cell.Value + B.Cell.Value, ' ', C.Cell.Value);
  WriteLn(TStore<Integer>.Created, ' ', TStore<string>.Created);
  A.Free;
  B.Free;
  C.Free;

  SetLength(Row, 2);
  Row[0] := 'x';
  Row[1] := 'y';
  WriteLn(Length(Row), Row[0], Row[1]);
  for I := 0 to 3 do
    Four[I] := I * I;
  WriteLn(Four[3]);
end.
