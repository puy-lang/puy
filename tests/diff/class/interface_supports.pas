program InterfaceSupports;

// Supports and `as` with interfaces a class lists after its parent (objects.md 7.6): the three
// forms of Supports (an interface value with an `out` variable, an object with and without one,
// a class reference and a class name), `as` from an interface value and from an object, and
// classes that inherit the interfaces of their parent. FPC needs GUIDs for `as` and Supports and
// `TInterfacedObject` for the methods of `IInterface`. The `out` variable is set to `nil` before
// each call (FPC does not release the old value of an untyped `out` parameter). Ancestor
// interfaces, `is`, `nil`, the conversions by type name and EInvalidCast are in
// golden/runtime/interface_casts and interface_cast_errors.

uses System.SysUtils;

type
  IGreeter = interface
    ['{6F1D8C2A-3B4E-4F50-9A61-7B8C9D0E1F21}']
    function Greet: string;
  end;

  ICounter = interface
    ['{0B7E5D3C-8A9F-4C21-B6E4-2D1F0A9C8B37}']
    function Next: Integer;
  end;

  TGreeter = class(TInterfacedObject, IGreeter)
  public
    function Greet: string;
  end;

  TBoth = class(TInterfacedObject, IGreeter, ICounter)
  private
    FN: Integer;
  public
    function Greet: string;
    function Next: Integer;
  end;

  TBothChild = class(TBoth)
  end;

function TGreeter.Greet: string;
begin
  Result := 'hello';
end;

function TBoth.Greet: string;
begin
  Result := 'both ' + IntToStr(FN);
end;

function TBoth.Next: Integer;
begin
  Inc(FN);
  Result := FN;
end;

var
  G: IGreeter;
  C: ICounter;
  B: TBothChild;
  O: TObject;
  K: TClass;
begin
  // An interface value: Supports with and without the `out` variable, and `as`
  G := TBoth.Create;
  C := nil;
  if Supports(G, ICounter, C) then
    WriteLn('counter ', C.Next, ' ', C.Next);
  C := G as ICounter;
  WriteLn(C.Next, ' ', (G as ICounter).Next, ' ', G.Greet);
  WriteLn(Supports(G, IGreeter), ' ', Supports(G, ICounter));
  G := TGreeter.Create;
  C := nil;
  WriteLn(Supports(G, ICounter, C), ' ', C = nil);
  WriteLn(Supports(G, IGreeter), ' ', Supports(G, ICounter));

  // An object (kept alive by an interface variable): Supports and `as`; the interfaces of the
  // parent class
  B := TBothChild.Create;
  G := B;
  O := B;
  C := nil;
  WriteLn(Supports(O, ICounter, C), ' ', C.Next);
  WriteLn(Supports(O, IGreeter), ' ', (O as IGreeter).Greet);
  WriteLn((B as ICounter).Next, ' ', G.Greet);

  // A class reference and a class name
  K := TBothChild;
  WriteLn(Supports(K, ICounter), ' ', Supports(K, IGreeter));
  K := TGreeter;
  WriteLn(Supports(K, ICounter), ' ', Supports(K, IGreeter));
  WriteLn(Supports(TBoth, ICounter), ' ', Supports(TGreeter, ICounter));
end.
