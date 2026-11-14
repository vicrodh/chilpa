import QtQuick
import QtTest
import "../../qml/Primitives"

TestCase {
    id:test
    name:"ArtTileAspect"
    when:windowShown
    visible:true
    width:600; height:400
    Rectangle { id:stage; width:240; height:240; color:"black" }
    Component { id:maker; ArtTile { width:100; height:100; radius:0; edged:false } }
    Canvas {
        id:landscape; x:300; width:200; height:100
        onPaint:{
            const context=getContext("2d")
            context.fillStyle="blue"; context.fillRect(0,0,width,height)
            context.fillStyle="red"; context.fillRect(0,0,width/4,height)
            context.fillStyle="lime"; context.fillRect(width*3/4,0,width/4,height)
        }
    }
    Canvas {
        id:portrait; x:300; y:150; width:100; height:200
        onPaint:{
            const context=getContext("2d")
            context.fillStyle="blue"; context.fillRect(0,0,width,height)
            context.fillStyle="red"; context.fillRect(0,0,width,height/4)
            context.fillStyle="lime"; context.fillRect(0,height*3/4,width,height/4)
        }
    }
    function cropped(source,width,height) {
        waitForRendering(source)
        const tile=createTemporaryObject(maker,stage,{width:width,height:height,source:source.toDataURL("image/png")})
        tryVerify(()=>tile.ready,2000,"the calibration image loads")
        waitForRendering(tile)
        const pixels=grabImage(tile)
        for (const fraction of [0.2,0.5,0.8]) {
            const x=Math.floor(width*fraction),y=Math.floor(height*fraction)
            verify(pixels.blue(x,y)>240 && pixels.red(x,y)<15 && pixels.green(x,y)<15,
                "cropping keeps the central blue field instead of squeezing the colored ends")
        }
    }
    function test_landscape_photos_fill_square_portraits_without_stretching() { cropped(landscape,100,100) }
    function test_portrait_photos_fill_a_wide_hero_without_stretching() { cropped(portrait,200,100) }
    function test_landscape_photos_fill_tall_tiles_without_stretching() { cropped(landscape,100,200) }
}
