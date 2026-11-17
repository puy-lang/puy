program GenericInheritance;

// Inheritance with instantiations (generics.md 2.4, 4.5): a generic class whose parent is an
// instantiation with its type parameters (`TSorted<T> = class(TBase<T>)`), a class that is not
// generic inheriting from an instantiation declared in the same section (`TIntList`), virtual
// calls through the parent's type and `inherited`, fields after the parent's, a generic class
// implementing a generic interface of its type parameters (`class(TRefObject, IShow<T>)`) and a
// descendant overriding its method, and an instantiation assigned to its parent's type and to
// the interface it implements.

uses System.SysUtils;

type
  IShow<T> = interface
    function Show(const X: T): string;
  end;

  TBase<T> = class(TRefObject, IShow<T>)
  protected
    FItems: TArray<T>;
  public
    procedure Add(const X: T);
    function Count: Integer;
    function Describe: string; virtual;
    function Show(const X: T): string; virtual;
  end;

  TSorted<T> = class(TBase<T>)
  public
    Label_: string;
    function Describe: string; override;
  end;

  TIntList = class(TBase<Integer>)
  public
    Extra: Integer;
    function Describe: string; override;
    function Show(const X: Integer): string; override;
  end;

procedure TBase<T>.Add(const X: T);
begin
  SetLength(FItems, Length(FItems) + 1);
  FItems[High(FItems)] := X;
end;

function TBase<T>.Count: Integer;
begin
  Result := Length(FItems);
end;

function TBase<T>.Describe: string;
begin
  Result := 'base ' + IntToStr(Count);
end;

function TBase<T>.Show(const X: T): string;
begin
  Result := 'item of ' + ClassName;
end;

function TSorted<T>.Describe: string;
begin
  Result := Label_ + 'sorted ' + inherited Describe;
end;

function TIntList.Describe: string;
begin
  Result := 'ints ' + IntToStr(Count) + ' ' + IntToStr(Extra) + ' ' + IntToStr(FItems[0]);
end;

function TIntList.Show(const X: Integer): string;
begin
  Result := 'int ' + IntToStr(X);
end;

var
  B: TBase<string>;
  S: TSorted<Double>;
  L: TIntList;
  BI: TBase<Integer>;
  I: IShow<Integer>;
  J: IShow<string>;
begin
  B := TSorted<string>.Create;
  B.Add('x');
  WriteLn(B.Describe);
  J := B;
  WriteLn(J.Show('y'));
  S := TSorted<Double>.Create;
  S.Label_ := '> ';
  S.Add(1.5);
  S.Add(2.5);
  WriteLn(S.Describe, ' ', S is TBase<Double>);
  L := TIntList.Create;
  L.Add(7);
  L.Extra := 9;
  BI := L;
  WriteLn(BI.Describe);
  I := L;
  WriteLn(I.Show(3));
  WriteLn(TBase<Integer>.Create.Describe);
end.
