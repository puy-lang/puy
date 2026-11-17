program ClassLeak;

uses System.SysUtils;

// Instances of classes managed by hand that are never freed are reported at the end by their
// class's fully qualified name, `TObject` by its own name (objects.md 3.3; diagnostics.md 8.1).
// The string a leaked instance holds is reported too; an instance made and thrown away in an
// expression statement leaks the same way (4.1).

type
  TNode = class
    Value: Integer;
  end;
  TNamed = class(TNode)
    Name: string;
  end;

var
  A, B: TNode;
  N: TNamed;
  O: TObject;
begin
  A := TNode.Create;
  B := TNode.Create;
  N := TNamed.Create;
  N.Name := 'kept' + IntToStr(A.Value);
  O := TObject.Create;
  TNode.Create;
  B.Free;
  WriteLn('end');
end.
