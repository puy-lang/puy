program InterfaceSyntax;

// The syntax of interfaces (objects.md 7.1, grammar.md 2.2): `INTERFACE_TYPE` with the parent,
// the `GUID_SPEC` and the `INTERFACE_BODY` of method headings and properties, the forward
// declaration and the form without members, and a `METHOD_RESOLUTION` in a class body.

type
  IFwd = interface;

  IFwd = interface(IInterface)
    ['{6B29FC40-CA47-1067-B31D-00DD010662DA}']
    function Area: Integer;
    procedure Scale(F: Integer); overload;
    property Size: Integer read Area;
  end;

  IEmpty = interface(IFwd);

  TImpl = class(TRefObject, IFwd)
    function IFwd.Area = DoArea;
    function DoArea: Integer;
    procedure Scale(F: Integer); overload;
  end;

function TImpl.DoArea: Integer;
begin
  Result := 0;
end;

procedure TImpl.Scale(F: Integer);
begin
end;

begin
end.
