program DumpInterface;

// The dumps of interfaces (objects.md 15.4): the `interface` type with its GUID, methods and
// properties, a class's interface table (a virtual method by its table entry, an abstract one),
// the call through an interface (`intfcall` / `CallIntf`) and the interface call target section.

type
  IShape = interface
    ['{6B29FC40-CA47-1067-B31D-00DD010662DA}']
    function Area: Integer;
    function GetName: string;
    property Name: string read GetName;
  end;

  IShape2 = interface(IShape)
    procedure Scale(F: Integer);
  end;

  TBase = class(TRefObject, IShape2)
  private
    function GetName: string;
  public
    function Area: Integer; virtual; abstract;
    procedure Scale(F: Integer);
  end;

  TSquare = class(TBase)
  public
    function Area: Integer; override;
  end;

function TBase.GetName: string;
begin
  Result := 'shape';
end;

procedure TBase.Scale(F: Integer);
begin
end;

function TSquare.Area: Integer;
begin
  Result := 4;
end;

var
  S: IShape2;
begin
  S := TSquare.Create;
  S.Scale(2);
  WriteLn(S.Name, ' ', S.Area);
end.
