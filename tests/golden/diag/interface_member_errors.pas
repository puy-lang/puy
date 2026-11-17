program InterfaceMemberErrors;

// Variants of the interface diagnostics not covered by golden/diag/interface_decl_errors and
// interface_impl_errors (objects.md 7.1, 7.2, 7.3, 7.8, 16.1): the words that cannot be written in
// an interface, a `var` section, a method resolution inside an interface, a calling convention
// (W0009) and `stored` (W0022); a method resolution naming something that is not an interface, one
// whose method has other parameters, and one written in a record (E0750); an interface method
// named `Free` that only `TObject` has (E0749); `TContainedObject` (E3020); the GUID routines and
// a `Supports` asking with a GUID (E3200).

uses
  System.SysUtils;

type
  IRun = interface
    procedure Run(X: Integer);
  end;

  IBad = interface
    destructor Destroy;
    class function Make: Integer;
    procedure A; static;
    procedure B; abstract;
    procedure C; dynamic;
    procedure D; reintroduce;
    procedure E; final;
    procedure F(var M: Integer); message 1;
    procedure G; dispid 1;
    procedure H; stdcall;
    function GetP: Integer;
    property P: Integer read GetP stored False;
    procedure IRun.Run = A;
  const
    K = 1;
  type
    TInner = Integer;
  var
    V: Integer;
  class var
    W: Integer;
  end;

  IFree = interface
    procedure Free;
  end;

  TImpl = class(TRefObject, IRun)
    procedure TObject.Run = DoRun;
    procedure IRun.Run = DoRun;
    procedure DoRun;
  end;

  TFreeImpl = class(TRefObject, IFree)
  end;

  TRec = record
    procedure IRun.Run = DoRun;
    procedure DoRun;
  end;

  TCont = class(TContainedObject)
  end;

procedure TImpl.DoRun;
begin
end;

procedure TRec.DoRun;
begin
end;

var
  O: TImpl;
  G: Boolean;
begin
  O := TImpl.Create;
  G := Supports(O, '{0F9A1B2C-3D4E-5F60-7182-93A4B5C6D7E8}');
  G := IsEqualGUID(1, 2);
  WriteLn(GUIDToString(1));
  WriteLn(StringToGUID(''));
  CreateGUID(G);
  WriteLn(G);
end.
