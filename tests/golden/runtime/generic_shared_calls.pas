program GenericSharedCalls;

// Every way a function is named still reaches the right one once the functions of instantiations
// are shared and the function table is renumbered (generics.md 7): calls, virtual method tables,
// interface tables, method references, anonymous methods, nested routines and generic routines.
// `TFoo` and `TBar` are classes managed by hand, so their instantiations share (the functions
// of `TCell<TFoo>`, made first, stand for both); `Pick<string>` does not share and moves down.
// The methods that do not depend on `T` are shared by `TCell<string>` too.

uses System.SysUtils;

type
  IShow = interface
    function Show: string;
  end;

  TBase = class(TRefObject)
    function Kind: string; virtual;
  end;

  TCell<T> = class(TBase, IShow)
    FValue: T;
    FCount: Integer;
    function Kind: string; override;
    function Show: string;
    procedure Bump;
    function Counter: TFunc<Integer>;
    function Twice: Integer;
  end;

  TFoo = class
  end;
  TBar = class
  end;

function TBase.Kind: string;
begin
  Result := 'base';
end;

function TCell<T>.Kind: string;
begin
  Result := 'cell ' + IntToStr(FCount);
end;

function TCell<T>.Show: string;
begin
  Result := 'show ' + IntToStr(FCount);
end;

procedure TCell<T>.Bump;
begin
  Inc(FCount);
end;

function TCell<T>.Counter: TFunc<Integer>;
begin
  Result :=
    function: Integer
    begin
      Inc(FCount);
      Result := FCount;
    end;
end;

function TCell<T>.Twice: Integer;

  function Add: Integer;
  begin
    Result := FCount + FCount;
  end;

begin
  Result := Add;
end;

function Pick<T>(const A, B: T; First: Boolean): T;
begin
  if First then
    Result := A
  else
    Result := B;
end;

var
  F: TCell<TFoo>;
  S: TCell<string>;
  B: TCell<TBar>;
  Base: TBase;
  Shown: IShow;
  P: TProc;
  C: TFunc<Integer>;
  Foo1, Foo2: TFoo;
  Bar1, Bar2: TBar;
begin
  F := TCell<TFoo>.Create;
  S := TCell<string>.Create;
  S.FValue := 'text';
  P := F.Bump;
  P();
  C := F.Counter();
  WriteLn(C(), ' ', F.Twice);
  B := TCell<TBar>.Create;
  B.Bump;
  Base := B;
  WriteLn(Base.Kind);
  Shown := B;
  WriteLn(Shown.Show);
  P := B.Bump;
  P();
  WriteLn(B.FCount);
  C := B.Counter();
  WriteLn(C(), ' ', C());
  WriteLn(B.Twice);
  WriteLn(S.Kind, ' ', S.Twice, ' ', S.FValue);
  WriteLn(F.Kind);
  Foo1 := TFoo.Create;
  Foo2 := TFoo.Create;
  Bar1 := TBar.Create;
  Bar2 := TBar.Create;
  WriteLn(Pick<TFoo>(Foo1, Foo2, True) = Foo1, ' ', Pick<TBar>(Bar1, Bar2, False) = Bar2,
    ' ', Pick<string>('a', 'b', False));
  Foo1.Free;
  Foo2.Free;
  Bar1.Free;
  Bar2.Free;
end.
