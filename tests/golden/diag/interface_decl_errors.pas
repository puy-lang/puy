program InterfaceDeclErrors;

// The declaration of an interface (objects.md 7.1, 16.1): members other than method headings and
// properties, method directives, constructors and class methods are E0750, as are a parent that
// is not an interface and a malformed GUID; one parent only (E0400); a method with an ancestor's
// name and parameters is E0701; a property reads and writes through the interface's methods
// (E0746); a forward declaration needs a body (E0723); `implements` is E3200.

type
  IBase = interface
    procedure Run;
  end;

  IMembers = interface(IBase)
    ['{0F9A1B2C-3D4E-5F60-7182-93A4B5C6D7E8}']
    FCount: Integer;
  private
    procedure Run;
    procedure Virt; virtual;
    procedure Over; override;
    constructor Create;
    class procedure Make;
    function GetCount: Integer;
    property Count: Integer read FCount;
    property Size: Integer read GetCount write SetSize;
    class property Total: Integer read GetCount;
    property Impl: IBase read GetBase implements IBase;
  end;

  ITwo = interface(IBase, IMembers)
  end;

  INotIntf = interface(TObject)
  end;

  IBadGuid = interface
    ['{1234}']
  end;

  IForward = interface;

begin
end.
